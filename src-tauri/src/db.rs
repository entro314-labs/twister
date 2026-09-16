//! The store: one SQLite file holding everything the capture hook has seen —
//! people and posts a site itself loaded into the page — plus the job
//! ledger and the scheduled posts.
//!
//! Nothing here is fetched. Twister never calls a network's API; the page
//! does, and the hook copies what comes back. That is why every row carries
//! a `network`, a `source` (the operation the page used: X's `Following`,
//! Bluesky's `app.bsky.feed.getAuthorFeed`, Meta's query names) and a
//! `last_seen`: a row is evidence of what the page showed, when. Ids are
//! only unique within a network — X and Meta both hand out numbers — so a
//! person or a post is keyed by both.
//!
//! WAL, so the MCP binary can read and write alongside the running app.

use std::path::Path;
use std::sync::Mutex;

use rusqlite::{Connection, OptionalExtension, params, params_from_iter, types::ToSql};
use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result, internal};
use crate::network::Network;

const SCHEMA_VERSION: i64 = 2;

pub const DB_FILE: &str = "twister.sqlite3";

/// One person, as the page last showed them. Optional relationship flags are
/// only present when X included them, which it does when signed in.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct User {
    pub network: Network,
    pub id: String,
    pub handle: String,
    pub name: String,
    pub bio: String,
    pub location: String,
    pub website: String,
    pub followers: i64,
    pub following: i64,
    pub posts: i64,
    pub verified: bool,
    pub protected: bool,
    pub avatar: String,
    /// RFC 3339, or empty when X did not say.
    pub created_at: String,
    /// They follow the signed-in account.
    pub follows_me: Option<bool>,
    /// The signed-in account follows them.
    pub followed_by_me: Option<bool>,
    pub source: String,
    pub first_seen: String,
    pub last_seen: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct Media {
    /// `photo` | `video` | `animated_gif`
    pub kind: String,
    /// The best URL: the original photo, or the highest-bitrate mp4.
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct Post {
    pub network: Network,
    pub id: String,
    /// The short id the network's URLs carry when it is not `id` itself:
    /// Meta's shortcode. Empty on X and Bluesky.
    pub slug: String,
    pub author_id: String,
    pub author_handle: String,
    pub text: String,
    pub created_at: String,
    /// `post` | `reply` | `repost` | `quote`
    pub kind: String,
    pub lang: String,
    pub likes: i64,
    pub reposts: i64,
    pub replies: i64,
    pub views: i64,
    pub bookmarked: bool,
    pub media: Vec<Media>,
    /// For a repost: the id of the post it repeats.
    pub repost_of: String,
    pub reply_to: String,
    pub quoted_id: String,
    pub source: String,
    pub first_seen: String,
    pub last_seen: String,
}

impl User {
    pub fn url(&self) -> String {
        self.network.profile_url(&self.handle)
    }
}

impl Post {
    pub fn url(&self) -> String {
        self.network
            .post_url(&self.author_handle, &self.id, &self.slug)
    }
}

/// How the People screen and the MCP tools narrow the users table. Every
/// field is optional; an empty filter is everyone, newest sighting first.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct UserFilter {
    pub network: Option<Network>,
    /// Matched against handle, name and bio, case-insensitively.
    pub search: String,
    pub source: Option<String>,
    pub follows_me: Option<bool>,
    pub followed_by_me: Option<bool>,
    pub verified: Option<bool>,
    pub min_followers: Option<i64>,
    pub max_followers: Option<i64>,
    pub min_posts: Option<i64>,
    /// `seen` (default) | `followers` | `handle`
    pub sort: String,
    pub limit: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct PostFilter {
    pub network: Option<Network>,
    pub search: String,
    pub source: Option<String>,
    pub kind: Option<String>,
    pub author: Option<String>,
    pub bookmarked: Option<bool>,
    pub has_media: Option<bool>,
    /// RFC 3339 bounds on `created_at`.
    pub since: Option<String>,
    pub until: Option<String>,
    /// `seen` (default) | `created` | `likes`
    pub sort: String,
    pub limit: Option<i64>,
}

/// One entry in the job ledger. Jobs run in the page (see `ops.rs`); the
/// ledger is what survives, and what the MCP binary writes to enqueue work.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct Job {
    pub id: i64,
    pub network: Network,
    pub kind: String,
    /// JSON, as given to the runner.
    pub params: String,
    /// `queued` | `running` | `done` | `failed` | `cancelled`
    pub status: String,
    pub dry_run: bool,
    pub total: i64,
    pub done: i64,
    pub skipped: i64,
    pub failed: i64,
    pub message: String,
    /// `app` | `mcp`
    pub origin: String,
    pub created_at: String,
    pub finished_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct ScheduledPost {
    pub id: i64,
    pub network: Network,
    /// The thread, one string per post.
    pub parts: Vec<String>,
    /// RFC 3339.
    pub scheduled_at: String,
    /// `scheduled` | `posting` | `posted` | `failed` | `missed`
    pub status: String,
    pub error: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Counts {
    pub users: i64,
    pub posts: i64,
    pub sources: Vec<(String, i64)>,
    /// Rows per network, most first — over everything, whatever the filter.
    pub networks: Vec<(String, i64)>,
}

/// A network as a column value, and back. A row from before networks, or
/// one a newer build wrote for a network this one lacks, reads as X.
fn network_column(slug: &str) -> Network {
    Network::parse(slug).unwrap_or_default()
}

pub struct Db {
    conn: Mutex<Connection>,
}

pub fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

impl Db {
    pub fn open_at(path: &Path) -> Result<Self> {
        let conn = Connection::open(path).map_err(|e| internal("Opening the store", e))?;
        Self::from_connection(conn)
    }

    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory().map_err(|e| internal("Opening the store", e))?;
        Self::from_connection(conn)
    }

    fn from_connection(conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "busy_timeout", 5000)?;
        let db = Self {
            conn: Mutex::new(conn),
        };
        db.migrate()?;
        Ok(db)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Connection> {
        // SQLite transactions are atomic, so a holder that panicked mid-write
        // left a consistent file; recovering beats taking the app down.
        self.conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// The schema ladder: each step runs once, in order, with the version
    /// written after it, so an interrupted upgrade resumes.
    fn migrate(&self) -> Result<()> {
        let conn = self.lock();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
        )?;
        let mut version: i64 = conn
            .query_row(
                "SELECT value FROM meta WHERE key = 'schema_version'",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
        while version < SCHEMA_VERSION {
            let next = version + 1;
            conn.execute_batch(step_sql(next))?;
            conn.execute(
                "INSERT INTO meta (key, value) VALUES ('schema_version', ?1)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![next.to_string()],
            )?;
            version = next;
        }
        Ok(())
    }

    // ─── Capture ────────────────────────────────────────────────────────────

    /// Upserts what the page saw. A later sighting overwrites counts and
    /// text; `first_seen` is kept, and a relationship flag X omitted this
    /// time does not erase one it sent before.
    pub fn record_users(&self, users: &[User]) -> Result<usize> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        let seen = now();
        for user in users {
            tx.execute(
                "INSERT INTO users (network, id, handle, name, bio, location, website, followers,
                    following, posts, verified, protected, avatar, created_at, follows_me,
                    followed_by_me, source, first_seen, last_seen)
                 VALUES (?18, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?17)
                 ON CONFLICT(network, id) DO UPDATE SET
                    handle = excluded.handle, name = excluded.name, bio = excluded.bio,
                    location = excluded.location, website = excluded.website,
                    followers = excluded.followers, following = excluded.following,
                    posts = excluded.posts, verified = excluded.verified,
                    protected = excluded.protected, avatar = excluded.avatar,
                    created_at = CASE WHEN excluded.created_at = '' THEN users.created_at ELSE excluded.created_at END,
                    follows_me = COALESCE(excluded.follows_me, users.follows_me),
                    followed_by_me = COALESCE(excluded.followed_by_me, users.followed_by_me),
                    source = excluded.source, last_seen = excluded.last_seen",
                params![
                    user.id,
                    user.handle,
                    user.name,
                    user.bio,
                    user.location,
                    user.website,
                    user.followers,
                    user.following,
                    user.posts,
                    user.verified,
                    user.protected,
                    user.avatar,
                    user.created_at,
                    user.follows_me,
                    user.followed_by_me,
                    user.source,
                    seen,
                    user.network.slug(),
                ],
            )?;
        }
        tx.commit()?;
        Ok(users.len())
    }

    pub fn record_posts(&self, posts: &[Post]) -> Result<usize> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        let seen = now();
        for post in posts {
            tx.execute(
                "INSERT INTO posts (network, slug, id, author_id, author_handle, text, created_at,
                    kind, lang, likes, reposts, replies, views, bookmarked, media, repost_of,
                    reply_to, quoted_id, source, first_seen, last_seen)
                 VALUES (?19, ?20, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?18)
                 ON CONFLICT(network, id) DO UPDATE SET
                    slug = CASE WHEN excluded.slug = '' THEN posts.slug ELSE excluded.slug END,
                    author_id = excluded.author_id, author_handle = excluded.author_handle,
                    text = excluded.text, created_at = excluded.created_at, kind = excluded.kind,
                    lang = excluded.lang, likes = excluded.likes, reposts = excluded.reposts,
                    replies = excluded.replies, views = excluded.views,
                    bookmarked = excluded.bookmarked, media = excluded.media,
                    repost_of = excluded.repost_of, reply_to = excluded.reply_to,
                    quoted_id = excluded.quoted_id,
                    -- A bookmarks sighting is the one worth keeping as the source.
                    source = CASE WHEN posts.source = 'Bookmarks' AND excluded.source != 'Bookmarks'
                                  THEN posts.source ELSE excluded.source END,
                    last_seen = excluded.last_seen",
                params![
                    post.id,
                    post.author_id,
                    post.author_handle,
                    post.text,
                    post.created_at,
                    post.kind,
                    post.lang,
                    post.likes,
                    post.reposts,
                    post.replies,
                    post.views,
                    post.bookmarked,
                    serde_json::to_string(&post.media)?,
                    post.repost_of,
                    post.reply_to,
                    post.quoted_id,
                    post.source,
                    seen,
                    post.network.slug(),
                    post.slug,
                ],
            )?;
        }
        tx.commit()?;
        Ok(posts.len())
    }

    // ─── Reads ──────────────────────────────────────────────────────────────

    /// How much is stored, on one network or on all of them.
    pub fn counts(&self, network: Option<Network>) -> Result<Counts> {
        let conn = self.lock();
        // `?1` is the slug, or an empty string for everything.
        let scope = network.map_or(String::new(), |n| n.slug().to_string());
        let users = conn.query_row(
            "SELECT COUNT(*) FROM users WHERE ?1 = '' OR network = ?1",
            params![scope],
            |row| row.get(0),
        )?;
        let posts = conn.query_row(
            "SELECT COUNT(*) FROM posts WHERE ?1 = '' OR network = ?1",
            params![scope],
            |row| row.get(0),
        )?;
        let mut statement = conn.prepare(
            "SELECT source, COUNT(*) AS n FROM (
                SELECT source FROM users WHERE ?1 = '' OR network = ?1
                UNION ALL SELECT source FROM posts WHERE ?1 = '' OR network = ?1
             ) GROUP BY source ORDER BY n DESC",
        )?;
        let sources = statement
            .query_map(params![scope], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let mut statement = conn.prepare(
            "SELECT network, COUNT(*) AS n FROM (
                SELECT network FROM users UNION ALL SELECT network FROM posts
             ) GROUP BY network ORDER BY n DESC",
        )?;
        let networks = statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(Counts {
            users,
            posts,
            sources,
            networks,
        })
    }

    pub fn users(&self, filter: &UserFilter) -> Result<Vec<User>> {
        let (clauses, values) = user_clauses(filter);
        let order = match filter.sort.as_str() {
            "followers" => "followers DESC",
            "handle" => "handle COLLATE NOCASE ASC",
            _ => "last_seen DESC, handle COLLATE NOCASE ASC",
        };
        let sql = format!(
            "SELECT id, handle, name, bio, location, website, followers, following, posts,
                verified, protected, avatar, created_at, follows_me, followed_by_me, source,
                first_seen, last_seen, network
             FROM users WHERE {} ORDER BY {order} LIMIT {}",
            clauses.join(" AND "),
            limit_of(filter.limit)
        );
        let conn = self.lock();
        let mut statement = conn.prepare(&sql)?;
        let rows = statement.query_map(params_from_iter(values.iter()), user_row)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn posts(&self, filter: &PostFilter) -> Result<Vec<Post>> {
        let (clauses, values) = post_clauses(filter);
        let order = match filter.sort.as_str() {
            "created" => "created_at DESC",
            "likes" => "likes DESC",
            _ => "last_seen DESC, created_at DESC",
        };
        let sql = format!(
            "SELECT id, author_id, author_handle, text, created_at, kind, lang, likes, reposts,
                replies, views, bookmarked, media, repost_of, reply_to, quoted_id, source,
                first_seen, last_seen, network, slug
             FROM posts WHERE {} ORDER BY {order} LIMIT {}",
            clauses.join(" AND "),
            limit_of(filter.limit)
        );
        let conn = self.lock();
        let mut statement = conn.prepare(&sql)?;
        let rows = statement.query_map(params_from_iter(values.iter()), post_row)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn post(&self, network: Network, id: &str) -> Result<Option<Post>> {
        let conn = self.lock();
        let found = conn
            .query_row(
                "SELECT id, author_id, author_handle, text, created_at, kind, lang, likes, reposts,
                    replies, views, bookmarked, media, repost_of, reply_to, quoted_id, source,
                    first_seen, last_seen, network, slug
                 FROM posts WHERE network = ?1 AND id = ?2",
                params![network.slug(), id],
                post_row,
            )
            .optional()?;
        Ok(found)
    }

    /// Forgets every captured person and post. The ledger and the schedule stay.
    pub fn clear_captured(&self) -> Result<()> {
        let conn = self.lock();
        conn.execute_batch("DELETE FROM users; DELETE FROM posts;")?;
        Ok(())
    }

    pub fn remove_posts(&self, network: Network, ids: &[String]) -> Result<usize> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        let mut removed = 0;
        for id in ids {
            removed += tx.execute(
                "DELETE FROM posts WHERE network = ?1 AND id = ?2",
                params![network.slug(), id],
            )?;
        }
        tx.commit()?;
        Ok(removed)
    }

    // ─── Jobs ───────────────────────────────────────────────────────────────

    pub fn create_job(
        &self,
        network: Network,
        kind: &str,
        params_json: &str,
        dry_run: bool,
        origin: &str,
    ) -> Result<Job> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO jobs (network, kind, params, status, dry_run, origin, created_at)
             VALUES (?6, ?1, ?2, 'queued', ?3, ?4, ?5)",
            params![kind, params_json, dry_run, origin, now(), network.slug()],
        )?;
        let id = conn.last_insert_rowid();
        drop(conn);
        self.job(id)?
            .ok_or_else(|| AppError::Internal("The job vanished on creation.".into()))
    }

    pub fn job(&self, id: i64) -> Result<Option<Job>> {
        let conn = self.lock();
        Ok(conn
            .query_row(
                "SELECT id, kind, params, status, dry_run, total, done, skipped, failed, message,
                    origin, created_at, finished_at, network FROM jobs WHERE id = ?1",
                params![id],
                job_row,
            )
            .optional()?)
    }

    pub fn jobs(&self, limit: i64) -> Result<Vec<Job>> {
        let conn = self.lock();
        let mut statement = conn.prepare(
            "SELECT id, kind, params, status, dry_run, total, done, skipped, failed, message,
                origin, created_at, finished_at, network FROM jobs ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = statement.query_map(params![limit], job_row)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// The oldest job still waiting, if any. What the app polls for work the
    /// MCP binary left it.
    pub fn next_queued_job(&self) -> Result<Option<Job>> {
        let conn = self.lock();
        Ok(conn
            .query_row(
                "SELECT id, kind, params, status, dry_run, total, done, skipped, failed, message,
                    origin, created_at, finished_at, network
                 FROM jobs WHERE status = 'queued' ORDER BY id ASC LIMIT 1",
                [],
                job_row,
            )
            .optional()?)
    }

    pub fn update_job(&self, job: &Job) -> Result<()> {
        let conn = self.lock();
        conn.execute(
            "UPDATE jobs SET status = ?2, total = ?3, done = ?4, skipped = ?5, failed = ?6,
                message = ?7, finished_at = ?8 WHERE id = ?1",
            params![
                job.id,
                job.status,
                job.total,
                job.done,
                job.skipped,
                job.failed,
                job.message,
                job.finished_at,
            ],
        )?;
        Ok(())
    }

    /// A job left `running` by a crash or a quit is not running any more.
    pub fn settle_stale_jobs(&self) -> Result<usize> {
        let conn = self.lock();
        Ok(conn.execute(
            "UPDATE jobs SET status = 'failed', message = 'Twister quit while this was running.',
                finished_at = ?1 WHERE status = 'running'",
            params![now()],
        )?)
    }

    // ─── Scheduled posts ────────────────────────────────────────────────────

    pub fn schedule_post(
        &self,
        network: Network,
        parts: &[String],
        scheduled_at: &str,
    ) -> Result<ScheduledPost> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO scheduled_posts (network, parts, scheduled_at, status, created_at)
             VALUES (?4, ?1, ?2, 'scheduled', ?3)",
            params![
                serde_json::to_string(parts)?,
                scheduled_at,
                now(),
                network.slug()
            ],
        )?;
        let id = conn.last_insert_rowid();
        drop(conn);
        self.scheduled_post(id)?
            .ok_or_else(|| AppError::Internal("The post vanished on creation.".into()))
    }

    pub fn scheduled_post(&self, id: i64) -> Result<Option<ScheduledPost>> {
        let conn = self.lock();
        Ok(conn
            .query_row(
                "SELECT id, parts, scheduled_at, status, error, created_at, network
                 FROM scheduled_posts WHERE id = ?1",
                params![id],
                scheduled_row,
            )
            .optional()?)
    }

    pub fn scheduled_posts(&self) -> Result<Vec<ScheduledPost>> {
        let conn = self.lock();
        let mut statement = conn.prepare(
            "SELECT id, parts, scheduled_at, status, error, created_at, network
             FROM scheduled_posts ORDER BY scheduled_at DESC LIMIT 200",
        )?;
        let rows = statement.query_map([], scheduled_row)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// Claims the next post whose time has come, flipping it to `posting` in
    /// the same statement so two passes cannot both take it.
    pub fn claim_due_post(&self, now_rfc3339: &str) -> Result<Option<ScheduledPost>> {
        let conn = self.lock();
        let id: Option<i64> = conn
            .query_row(
                "SELECT id FROM scheduled_posts
                 WHERE status = 'scheduled' AND scheduled_at <= ?1
                 ORDER BY scheduled_at ASC LIMIT 1",
                params![now_rfc3339],
                |row| row.get(0),
            )
            .optional()?;
        let Some(id) = id else {
            return Ok(None);
        };
        conn.execute(
            "UPDATE scheduled_posts SET status = 'posting' WHERE id = ?1",
            params![id],
        )?;
        drop(conn);
        self.scheduled_post(id)
    }

    pub fn settle_post(&self, id: i64, status: &str, error: &str) -> Result<()> {
        let conn = self.lock();
        conn.execute(
            "UPDATE scheduled_posts SET status = ?2, error = ?3 WHERE id = ?1",
            params![id, status, error],
        )?;
        Ok(())
    }

    /// Posts whose time passed by more than the grace window while the app
    /// was not running are marked `missed` rather than sent late.
    pub fn mark_missed(&self, before_rfc3339: &str) -> Result<usize> {
        let conn = self.lock();
        Ok(conn.execute(
            "UPDATE scheduled_posts SET status = 'missed',
                error = 'Twister was not running when this was due.'
             WHERE status IN ('scheduled', 'posting') AND scheduled_at < ?1",
            params![before_rfc3339],
        )?)
    }

    pub fn delete_scheduled_post(&self, id: i64) -> Result<()> {
        let conn = self.lock();
        conn.execute("DELETE FROM scheduled_posts WHERE id = ?1", params![id])?;
        Ok(())
    }
}

fn user_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<User> {
    let network: String = row.get(18)?;
    Ok(User {
        network: network_column(&network),
        id: row.get(0)?,
        handle: row.get(1)?,
        name: row.get(2)?,
        bio: row.get(3)?,
        location: row.get(4)?,
        website: row.get(5)?,
        followers: row.get(6)?,
        following: row.get(7)?,
        posts: row.get(8)?,
        verified: row.get(9)?,
        protected: row.get(10)?,
        avatar: row.get(11)?,
        created_at: row.get(12)?,
        follows_me: row.get(13)?,
        followed_by_me: row.get(14)?,
        source: row.get(15)?,
        first_seen: row.get(16)?,
        last_seen: row.get(17)?,
    })
}

fn post_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Post> {
    let media: String = row.get(12)?;
    let network: String = row.get(19)?;
    Ok(Post {
        network: network_column(&network),
        id: row.get(0)?,
        slug: row.get(20)?,
        author_id: row.get(1)?,
        author_handle: row.get(2)?,
        text: row.get(3)?,
        created_at: row.get(4)?,
        kind: row.get(5)?,
        lang: row.get(6)?,
        likes: row.get(7)?,
        reposts: row.get(8)?,
        replies: row.get(9)?,
        views: row.get(10)?,
        bookmarked: row.get(11)?,
        media: serde_json::from_str(&media).unwrap_or_default(),
        repost_of: row.get(13)?,
        reply_to: row.get(14)?,
        quoted_id: row.get(15)?,
        source: row.get(16)?,
        first_seen: row.get(17)?,
        last_seen: row.get(18)?,
    })
}

fn job_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Job> {
    let network: String = row.get(13)?;
    Ok(Job {
        id: row.get(0)?,
        network: network_column(&network),
        kind: row.get(1)?,
        params: row.get(2)?,
        status: row.get(3)?,
        dry_run: row.get(4)?,
        total: row.get(5)?,
        done: row.get(6)?,
        skipped: row.get(7)?,
        failed: row.get(8)?,
        message: row.get(9)?,
        origin: row.get(10)?,
        created_at: row.get(11)?,
        finished_at: row.get(12)?,
    })
}

fn scheduled_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ScheduledPost> {
    let parts: String = row.get(1)?;
    let network: String = row.get(6)?;
    Ok(ScheduledPost {
        id: row.get(0)?,
        network: network_column(&network),
        parts: serde_json::from_str(&parts).unwrap_or_default(),
        scheduled_at: row.get(2)?,
        status: row.get(3)?,
        error: row.get(4)?,
        created_at: row.get(5)?,
    })
}

fn limit_of(limit: Option<i64>) -> i64 {
    limit.map_or(500, |l| l.clamp(1, 100_000))
}

/// A LIKE pattern for a substring search, with the pattern characters escaped
/// so a `%` in what someone typed matches a percent sign.
fn like(search: &str) -> String {
    let escaped = search
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("%{escaped}%")
}

type Clauses = (Vec<String>, Vec<Box<dyn ToSql>>);

/// A WHERE clause built one condition at a time, each bound to the next
/// numbered parameter.
#[derive(Default)]
struct Where {
    clauses: Vec<String>,
    values: Vec<Box<dyn ToSql>>,
}

impl Where {
    fn push(&mut self, clause: &str, value: Box<dyn ToSql>) {
        self.values.push(value);
        self.clauses
            .push(clause.replace('?', &format!("?{}", self.values.len())));
    }

    fn finish(mut self) -> Clauses {
        if self.clauses.is_empty() {
            self.clauses.push("1 = 1".into());
        }
        (self.clauses, self.values)
    }
}

fn user_clauses(filter: &UserFilter) -> Clauses {
    let mut w = Where::default();
    if let Some(network) = filter.network {
        w.push("network = ?", Box::new(network.slug()));
    }
    let search = filter.search.trim();
    if !search.is_empty() {
        w.push(
            "(handle LIKE ? ESCAPE '\\' OR name LIKE ? ESCAPE '\\' OR bio LIKE ? ESCAPE '\\')",
            Box::new(like(search)),
        );
    }
    if let Some(source) = &filter.source
        && !source.is_empty()
    {
        w.push("source = ?", Box::new(source.clone()));
    }
    if let Some(v) = filter.follows_me {
        w.push("follows_me = ?", Box::new(v));
    }
    if let Some(v) = filter.followed_by_me {
        w.push("followed_by_me = ?", Box::new(v));
    }
    if let Some(v) = filter.verified {
        w.push("verified = ?", Box::new(v));
    }
    if let Some(v) = filter.min_followers {
        w.push("followers >= ?", Box::new(v));
    }
    if let Some(v) = filter.max_followers {
        w.push("followers <= ?", Box::new(v));
    }
    if let Some(v) = filter.min_posts {
        w.push("posts >= ?", Box::new(v));
    }
    w.finish()
}

fn post_clauses(filter: &PostFilter) -> Clauses {
    let mut w = Where::default();
    if let Some(network) = filter.network {
        w.push("network = ?", Box::new(network.slug()));
    }
    let search = filter.search.trim();
    if !search.is_empty() {
        w.push("text LIKE ? ESCAPE '\\'", Box::new(like(search)));
    }
    if let Some(source) = &filter.source
        && !source.is_empty()
    {
        w.push("source = ?", Box::new(source.clone()));
    }
    if let Some(kind) = &filter.kind
        && !kind.is_empty()
    {
        w.push("kind = ?", Box::new(kind.clone()));
    }
    if let Some(author) = &filter.author
        && !author.is_empty()
    {
        w.push(
            "author_handle = ? COLLATE NOCASE",
            Box::new(author.trim_start_matches('@').to_string()),
        );
    }
    if let Some(v) = filter.bookmarked {
        w.push("bookmarked = ?", Box::new(v));
    }
    if let Some(v) = filter.has_media {
        w.clauses.push(if v {
            "media != '[]'".into()
        } else {
            "media = '[]'".into()
        });
    }
    if let Some(since) = &filter.since
        && !since.is_empty()
    {
        w.push("created_at >= ?", Box::new(since.clone()));
    }
    if let Some(until) = &filter.until
        && !until.is_empty()
    {
        w.push("created_at <= ?", Box::new(until.clone()));
    }
    w.finish()
}

// The whole ladder in one place, one step per version: splitting it would
// only scatter the schema.
#[allow(clippy::too_many_lines)]
fn step_sql(version: i64) -> &'static str {
    match version {
        1 => {
            "CREATE TABLE users (
                id TEXT PRIMARY KEY,
                handle TEXT NOT NULL,
                name TEXT NOT NULL DEFAULT '',
                bio TEXT NOT NULL DEFAULT '',
                location TEXT NOT NULL DEFAULT '',
                website TEXT NOT NULL DEFAULT '',
                followers INTEGER NOT NULL DEFAULT 0,
                following INTEGER NOT NULL DEFAULT 0,
                posts INTEGER NOT NULL DEFAULT 0,
                verified INTEGER NOT NULL DEFAULT 0,
                protected INTEGER NOT NULL DEFAULT 0,
                avatar TEXT NOT NULL DEFAULT '',
                created_at TEXT NOT NULL DEFAULT '',
                follows_me INTEGER,
                followed_by_me INTEGER,
                source TEXT NOT NULL DEFAULT '',
                first_seen TEXT NOT NULL,
                last_seen TEXT NOT NULL
            );
            CREATE INDEX users_handle ON users(handle COLLATE NOCASE);
            CREATE INDEX users_seen ON users(last_seen);
            CREATE TABLE posts (
                id TEXT PRIMARY KEY,
                author_id TEXT NOT NULL DEFAULT '',
                author_handle TEXT NOT NULL DEFAULT '',
                text TEXT NOT NULL DEFAULT '',
                created_at TEXT NOT NULL DEFAULT '',
                kind TEXT NOT NULL DEFAULT 'post',
                lang TEXT NOT NULL DEFAULT '',
                likes INTEGER NOT NULL DEFAULT 0,
                reposts INTEGER NOT NULL DEFAULT 0,
                replies INTEGER NOT NULL DEFAULT 0,
                views INTEGER NOT NULL DEFAULT 0,
                bookmarked INTEGER NOT NULL DEFAULT 0,
                media TEXT NOT NULL DEFAULT '[]',
                repost_of TEXT NOT NULL DEFAULT '',
                reply_to TEXT NOT NULL DEFAULT '',
                quoted_id TEXT NOT NULL DEFAULT '',
                source TEXT NOT NULL DEFAULT '',
                first_seen TEXT NOT NULL,
                last_seen TEXT NOT NULL
            );
            CREATE INDEX posts_author ON posts(author_handle COLLATE NOCASE);
            CREATE INDEX posts_seen ON posts(last_seen);
            CREATE INDEX posts_source ON posts(source);
            CREATE TABLE jobs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                kind TEXT NOT NULL,
                params TEXT NOT NULL DEFAULT '{}',
                status TEXT NOT NULL DEFAULT 'queued',
                dry_run INTEGER NOT NULL DEFAULT 1,
                total INTEGER NOT NULL DEFAULT 0,
                done INTEGER NOT NULL DEFAULT 0,
                skipped INTEGER NOT NULL DEFAULT 0,
                failed INTEGER NOT NULL DEFAULT 0,
                message TEXT NOT NULL DEFAULT '',
                origin TEXT NOT NULL DEFAULT 'app',
                created_at TEXT NOT NULL,
                finished_at TEXT NOT NULL DEFAULT ''
            );
            CREATE TABLE scheduled_posts (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                parts TEXT NOT NULL,
                scheduled_at TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'scheduled',
                error TEXT NOT NULL DEFAULT '',
                created_at TEXT NOT NULL
            );
            CREATE INDEX scheduled_due ON scheduled_posts(status, scheduled_at);"
        }
        // Networks. A person or a post is keyed by (network, id), which SQLite
        // cannot add to a table in place, so both are rebuilt with every row
        // marked as X's — the only network there was. Posts gain the URL slug
        // Meta's shortcodes need; jobs and the schedule gain a network.
        2 => {
            "CREATE TABLE users_v2 (
                network TEXT NOT NULL DEFAULT 'x',
                id TEXT NOT NULL,
                handle TEXT NOT NULL,
                name TEXT NOT NULL DEFAULT '',
                bio TEXT NOT NULL DEFAULT '',
                location TEXT NOT NULL DEFAULT '',
                website TEXT NOT NULL DEFAULT '',
                followers INTEGER NOT NULL DEFAULT 0,
                following INTEGER NOT NULL DEFAULT 0,
                posts INTEGER NOT NULL DEFAULT 0,
                verified INTEGER NOT NULL DEFAULT 0,
                protected INTEGER NOT NULL DEFAULT 0,
                avatar TEXT NOT NULL DEFAULT '',
                created_at TEXT NOT NULL DEFAULT '',
                follows_me INTEGER,
                followed_by_me INTEGER,
                source TEXT NOT NULL DEFAULT '',
                first_seen TEXT NOT NULL,
                last_seen TEXT NOT NULL,
                PRIMARY KEY (network, id)
            );
            INSERT INTO users_v2 (network, id, handle, name, bio, location, website, followers,
                following, posts, verified, protected, avatar, created_at, follows_me,
                followed_by_me, source, first_seen, last_seen)
             SELECT 'x', id, handle, name, bio, location, website, followers, following, posts,
                verified, protected, avatar, created_at, follows_me, followed_by_me, source,
                first_seen, last_seen FROM users;
            DROP TABLE users;
            ALTER TABLE users_v2 RENAME TO users;
            CREATE INDEX users_handle ON users(handle COLLATE NOCASE);
            CREATE INDEX users_seen ON users(last_seen);
            CREATE TABLE posts_v2 (
                network TEXT NOT NULL DEFAULT 'x',
                id TEXT NOT NULL,
                slug TEXT NOT NULL DEFAULT '',
                author_id TEXT NOT NULL DEFAULT '',
                author_handle TEXT NOT NULL DEFAULT '',
                text TEXT NOT NULL DEFAULT '',
                created_at TEXT NOT NULL DEFAULT '',
                kind TEXT NOT NULL DEFAULT 'post',
                lang TEXT NOT NULL DEFAULT '',
                likes INTEGER NOT NULL DEFAULT 0,
                reposts INTEGER NOT NULL DEFAULT 0,
                replies INTEGER NOT NULL DEFAULT 0,
                views INTEGER NOT NULL DEFAULT 0,
                bookmarked INTEGER NOT NULL DEFAULT 0,
                media TEXT NOT NULL DEFAULT '[]',
                repost_of TEXT NOT NULL DEFAULT '',
                reply_to TEXT NOT NULL DEFAULT '',
                quoted_id TEXT NOT NULL DEFAULT '',
                source TEXT NOT NULL DEFAULT '',
                first_seen TEXT NOT NULL,
                last_seen TEXT NOT NULL,
                PRIMARY KEY (network, id)
            );
            INSERT INTO posts_v2 (network, id, author_id, author_handle, text, created_at, kind,
                lang, likes, reposts, replies, views, bookmarked, media, repost_of, reply_to,
                quoted_id, source, first_seen, last_seen)
             SELECT 'x', id, author_id, author_handle, text, created_at, kind, lang, likes,
                reposts, replies, views, bookmarked, media, repost_of, reply_to, quoted_id,
                source, first_seen, last_seen FROM posts;
            DROP TABLE posts;
            ALTER TABLE posts_v2 RENAME TO posts;
            CREATE INDEX posts_author ON posts(author_handle COLLATE NOCASE);
            CREATE INDEX posts_seen ON posts(last_seen);
            CREATE INDEX posts_source ON posts(source);
            ALTER TABLE jobs ADD COLUMN network TEXT NOT NULL DEFAULT 'x';
            ALTER TABLE scheduled_posts ADD COLUMN network TEXT NOT NULL DEFAULT 'x';"
        }
        _ => unreachable!("no schema step {version}"),
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(err: rusqlite::Error) -> Self {
        Self::Internal(format!("Store error: {err}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user(id: &str, handle: &str) -> User {
        User {
            id: id.into(),
            handle: handle.into(),
            name: handle.to_uppercase(),
            source: "Following".into(),
            ..User::default()
        }
    }

    #[test]
    fn the_first_schema_migrates_with_every_row_kept_as_x() {
        let conn = Connection::open_in_memory().expect("conn");
        conn.execute_batch(
            "CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             INSERT INTO meta VALUES ('schema_version', '1');",
        )
        .expect("meta");
        conn.execute_batch(step_sql(1)).expect("v1");
        conn.execute_batch(
            "INSERT INTO users (id, handle, first_seen, last_seen) VALUES ('1', 'alice', 't', 't');
             INSERT INTO posts (id, author_handle, text, first_seen, last_seen)
                VALUES ('10', 'alice', 'hi', 't', 't');
             INSERT INTO jobs (kind, created_at) VALUES ('scan', 't');
             INSERT INTO scheduled_posts (parts, scheduled_at, created_at)
                VALUES ('[\"x\"]', '2030-01-01T00:00:00Z', 't');",
        )
        .expect("rows");
        let db = Db::from_connection(conn).expect("migrates");
        let users = db.users(&UserFilter::default()).expect("reads");
        assert_eq!(users.len(), 1);
        assert_eq!(users[0].network, Network::X);
        assert_eq!(users[0].handle, "alice");
        let posts = db.posts(&PostFilter::default()).expect("reads");
        assert_eq!(posts[0].network, Network::X);
        assert_eq!(posts[0].slug, "");
        assert_eq!(posts[0].url(), "https://x.com/alice/status/10");
        assert_eq!(db.jobs(10).expect("jobs")[0].network, Network::X);
        assert_eq!(
            db.scheduled_posts().expect("schedule")[0].network,
            Network::X
        );
        // The same id on another network is another row.
        db.record_users(&[User {
            network: Network::Threads,
            id: "1".into(),
            handle: "zuck".into(),
            ..User::default()
        }])
        .expect("records");
        assert_eq!(db.users(&UserFilter::default()).expect("reads").len(), 2);
        let threads = db
            .users(&UserFilter {
                network: Some(Network::Threads),
                ..UserFilter::default()
            })
            .expect("reads");
        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].url(), "https://www.threads.com/@zuck");
        let counts = db.counts(None).expect("counts");
        assert_eq!(counts.users, 2);
        assert_eq!(counts.networks.len(), 2);
        assert_eq!(db.counts(Some(Network::Threads)).expect("counts").users, 1);
    }

    #[test]
    fn users_upsert_and_keep_first_seen_and_known_flags() {
        let db = Db::open_in_memory().expect("db");
        let mut first = user("1", "alice");
        first.follows_me = Some(true);
        db.record_users(&[first]).expect("records");
        let seen = db.users(&UserFilter::default()).expect("reads");
        assert_eq!(seen.len(), 1);
        let first_seen = seen[0].first_seen.clone();

        let mut again = user("1", "alice_renamed");
        again.followers = 42;
        db.record_users(&[again]).expect("records");
        let seen = db.users(&UserFilter::default()).expect("reads");
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].handle, "alice_renamed");
        assert_eq!(seen[0].followers, 42);
        assert_eq!(seen[0].first_seen, first_seen);
        // The second sighting said nothing about the relationship; the first stands.
        assert_eq!(seen[0].follows_me, Some(true));
    }

    #[test]
    fn user_filters_narrow_by_search_flags_and_counts() {
        let db = Db::open_in_memory().expect("db");
        let mut a = user("1", "alice");
        a.bio = "Rust and 100% coffee".into();
        a.followers = 10;
        a.follows_me = Some(false);
        a.followed_by_me = Some(true);
        let mut b = user("2", "bob");
        b.followers = 1000;
        b.follows_me = Some(true);
        b.followed_by_me = Some(true);
        b.verified = true;
        db.record_users(&[a, b]).expect("records");

        let rust = db
            .users(&UserFilter {
                search: "rust".into(),
                ..UserFilter::default()
            })
            .expect("reads");
        assert_eq!(rust.len(), 1);
        assert_eq!(rust[0].handle, "alice");

        // `%` in the search is a literal, not a wildcard.
        let percent = db
            .users(&UserFilter {
                search: "100%".into(),
                ..UserFilter::default()
            })
            .expect("reads");
        assert_eq!(percent.len(), 1);

        let not_back = db
            .users(&UserFilter {
                followed_by_me: Some(true),
                follows_me: Some(false),
                ..UserFilter::default()
            })
            .expect("reads");
        assert_eq!(not_back.len(), 1);
        assert_eq!(not_back[0].handle, "alice");

        let big = db
            .users(&UserFilter {
                min_followers: Some(500),
                verified: Some(true),
                sort: "followers".into(),
                ..UserFilter::default()
            })
            .expect("reads");
        assert_eq!(big.len(), 1);
        assert_eq!(big[0].handle, "bob");
    }

    #[test]
    fn posts_round_trip_media_and_keep_the_bookmarks_source() {
        let db = Db::open_in_memory().expect("db");
        let post = Post {
            id: "10".into(),
            author_handle: "alice".into(),
            text: "hello".into(),
            kind: "post".into(),
            source: "Bookmarks".into(),
            bookmarked: true,
            media: vec![Media {
                kind: "photo".into(),
                url: "https://pbs.twimg.com/media/a.jpg?name=orig".into(),
            }],
            ..Post::default()
        };
        db.record_posts(std::slice::from_ref(&post))
            .expect("records");
        let again = Post {
            source: "HomeTimeline".into(),
            likes: 3,
            ..post
        };
        db.record_posts(&[again]).expect("records");
        let read = db.posts(&PostFilter::default()).expect("reads");
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].source, "Bookmarks");
        assert_eq!(read[0].likes, 3);
        assert_eq!(read[0].media.len(), 1);
        assert_eq!(read[0].url(), "https://x.com/alice/status/10");

        let with_media = db
            .posts(&PostFilter {
                has_media: Some(true),
                ..PostFilter::default()
            })
            .expect("reads");
        assert_eq!(with_media.len(), 1);
        let by_author = db
            .posts(&PostFilter {
                author: Some("@ALICE".into()),
                kind: Some("post".into()),
                ..PostFilter::default()
            })
            .expect("reads");
        assert_eq!(by_author.len(), 1);
        assert!(db.post(Network::X, "10").expect("reads").is_some());
        assert!(db.post(Network::X, "11").expect("reads").is_none());
        assert!(db.post(Network::Bluesky, "10").expect("reads").is_none());

        let counts = db.counts(None).expect("counts");
        assert_eq!(counts.posts, 1);
        assert_eq!(
            db.remove_posts(Network::Bluesky, &["10".into()])
                .expect("removes"),
            0
        );
        assert_eq!(
            db.remove_posts(Network::X, &["10".into()])
                .expect("removes"),
            1
        );
        db.clear_captured().expect("clears");
        assert_eq!(db.counts(None).expect("counts").posts, 0);
    }

    #[test]
    fn jobs_are_queued_claimed_and_settled() {
        let db = Db::open_in_memory().expect("db");
        let job = db
            .create_job(Network::Bluesky, "scan", "{}", true, "mcp")
            .expect("creates");
        assert_eq!(job.status, "queued");
        assert_eq!(job.network, Network::Bluesky);
        let next = db.next_queued_job().expect("reads").expect("one");
        assert_eq!(next.id, job.id);
        let running = Job {
            status: "running".into(),
            ..next
        };
        db.update_job(&running).expect("updates");
        assert!(db.next_queued_job().expect("reads").is_none());
        assert_eq!(db.settle_stale_jobs().expect("settles"), 1);
        assert_eq!(
            db.job(job.id).expect("reads").expect("one").status,
            "failed"
        );
        assert_eq!(db.jobs(10).expect("lists").len(), 1);
    }

    #[test]
    fn scheduled_posts_are_claimed_once_and_missed_when_stale() {
        let db = Db::open_in_memory().expect("db");
        let post = db
            .schedule_post(
                Network::X,
                &["one".into(), "two".into()],
                "2026-01-01T09:00:00Z",
            )
            .expect("schedules");
        assert_eq!(post.parts.len(), 2);
        assert_eq!(post.network, Network::X);
        assert!(
            db.claim_due_post("2026-01-01T08:00:00Z")
                .expect("claims")
                .is_none()
        );
        let claimed = db
            .claim_due_post("2026-01-01T09:00:30Z")
            .expect("claims")
            .expect("one");
        assert_eq!(claimed.status, "posting");
        assert!(
            db.claim_due_post("2026-01-01T09:00:30Z")
                .expect("claims")
                .is_none()
        );
        db.settle_post(claimed.id, "posted", "").expect("settles");
        let later = db
            .schedule_post(Network::Bluesky, &["late".into()], "2026-01-01T10:00:00Z")
            .expect("schedules");
        assert_eq!(db.mark_missed("2026-01-01T11:00:00Z").expect("marks"), 1);
        assert_eq!(
            db.scheduled_post(later.id)
                .expect("reads")
                .expect("one")
                .status,
            "missed"
        );
        db.delete_scheduled_post(later.id).expect("deletes");
        assert_eq!(db.scheduled_posts().expect("lists").len(), 1);
    }
}
