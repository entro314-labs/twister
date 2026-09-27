//! The agent door: an MCP server over the same store the app uses.
//!
//! Run as `twister-mcp`, a stdio JSON-RPC 2.0 server any agent host can
//! spawn (`claude mcp add twister -- /path/to/twister-mcp`). It reads what
//! the capture hook has stored, exports it, and queues operations — a scan
//! of a list, a follow run, a post — that the app picks up on its next tick.
//! Nothing here touches X: the store is the only thing this process opens,
//! and WAL lets it sit alongside the running app.
//!
//! Every queued job is a dry run unless the caller says otherwise, and the
//! answer to `queue_job` says the app has to be running for it to happen.

use std::sync::Arc;

use serde_json::{Value, json};

use crate::db::{self, Db, PostFilter, UserFilter};
use crate::error::{AppError, Result};
use crate::export::{self, Format};
use crate::network::{self, Network};
use crate::{ops, scheduler};

/// The modern revision: no handshake, every request names its version in
/// `_meta`. A client on it may call anything straight away.
const MODERN: &str = "2026-07-28";
/// Handshake-era revisions, newest first, served to a client that opens with
/// `initialize` — which is every client that predates the modern one.
const LEGACY: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];
const META_VERSION: &str = "io.modelcontextprotocol/protocolVersion";
const META_SERVER: &str = "io.modelcontextprotocol/serverInfo";
/// How long a client may keep the tool list: it only changes with a build.
const LIST_TTL_MS: u64 = 3_600_000;

const INSTRUCTIONS: &str = "Twister is a desktop client for X, Bluesky, Threads and Instagram. \
    This server reads the people and posts the app has seen each site load (it never calls an \
    API itself) and queues operations the app runs in its signed-in page on one network. Queued \
    jobs run only while the Twister app is open, one at a time, and default to dry runs; Threads \
    and Instagram accept scans only. Every row carries a `network` (x, bluesky, threads, \
    instagram). Call store_summary first to learn what has been captured, where, and from which \
    source.";

fn supported_versions() -> Vec<&'static str> {
    std::iter::once(MODERN)
        .chain(LEGACY.iter().copied())
        .collect()
}

fn server_info() -> Value {
    json!({ "name": "twister", "version": env!("CARGO_PKG_VERSION") })
}

fn error(id: &Value, code: i64, message: &str, data: Option<Value>) -> Value {
    let mut error = json!({ "code": code, "message": message });
    if let Some(data) = data {
        error["data"] = data;
    }
    json!({ "jsonrpc": "2.0", "id": id, "error": error })
}

/// The answer to a frame that is not JSON at all.
pub fn parse_error() -> Value {
    error(&Value::Null, -32700, "Parse error", None)
}

pub struct Session {
    db: Arc<Db>,
}

impl Session {
    pub fn new(db: Arc<Db>) -> Self {
        Self { db }
    }

    /// One frame. `None` for a notification or a response, neither of which
    /// is answered.
    ///
    /// Both eras of the protocol are served. A client that opens with
    /// `initialize` gets the newest handshake revision it asked for; a modern
    /// one names its revision in each request's `_meta` and is refused with
    /// the supported list when it names one this server does not speak.
    /// Results carry the modern fields (`resultType`, the server's identity,
    /// cache hints on lists), which a handshake-era client ignores.
    pub fn handle(&self, message: &Value) -> Option<Value> {
        let id = message.get("id").cloned()?;
        let method = message.get("method").and_then(Value::as_str)?;
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        if method != "initialize"
            && let Some(requested) = params
                .get("_meta")
                .and_then(|meta| meta.get(META_VERSION))
                .and_then(Value::as_str)
            && requested != MODERN
        {
            return Some(error(
                &id,
                -32022,
                "Unsupported protocol version",
                Some(json!({ "supported": supported_versions(), "requested": requested })),
            ));
        }
        let mut result = match method {
            "initialize" => {
                let requested = params
                    .get("protocolVersion")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let version = LEGACY
                    .iter()
                    .copied()
                    .find(|v| *v == requested)
                    .unwrap_or(LEGACY[0]);
                json!({
                    "protocolVersion": version,
                    "capabilities": { "tools": { "listChanged": false } },
                    "serverInfo": server_info(),
                    "instructions": INSTRUCTIONS,
                })
            }
            "server/discover" => json!({
                "supportedVersions": supported_versions(),
                "capabilities": { "tools": {} },
                "instructions": INSTRUCTIONS,
                "ttlMs": LIST_TTL_MS,
                "cacheScope": "public",
            }),
            "tools/list" => json!({
                "tools": tools(),
                "ttlMs": LIST_TTL_MS,
                "cacheScope": "public",
            }),
            "tools/call" => {
                let name = params.get("name").and_then(Value::as_str).unwrap_or("");
                if !tools().iter().any(|tool| tool["name"] == name) {
                    return Some(error(&id, -32602, &format!("Unknown tool `{name}`"), None));
                }
                match self.call(name, &params) {
                    Ok(result) => result,
                    Err(err) => json!({
                        "isError": true,
                        "content": [{ "type": "text", "text": err.message() }],
                    }),
                }
            }
            // Handshake-era only; the modern revision retired it.
            "ping" => json!({}),
            other => {
                return Some(error(
                    &id,
                    -32601,
                    &format!("Unknown method `{other}`"),
                    None,
                ));
            }
        };
        if method != "initialize" {
            result["resultType"] = json!("complete");
            result["_meta"] = json!({ META_SERVER: server_info() });
        }
        Some(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
    }

    fn call(&self, name: &str, params: &Value) -> Result<Value> {
        let args = params.get("arguments").cloned().unwrap_or(json!({}));
        let text = match name {
            "store_summary" => self.summary()?,
            "search_people" => {
                let filter: UserFilter = serde_json::from_value(args)?;
                serde_json::to_string_pretty(&self.db.users(&filter)?)?
            }
            "list_posts" => {
                let filter: PostFilter = serde_json::from_value(args)?;
                serde_json::to_string_pretty(&self.db.posts(&filter)?)?
            }
            "export" => self.export(&args)?,
            "queue_job" => self.queue(&args)?,
            "list_jobs" => serde_json::to_string_pretty(&self.db.jobs(30)?)?,
            "schedule_post" => self.schedule(&args)?,
            "list_scheduled_posts" => serde_json::to_string_pretty(&self.db.scheduled_posts()?)?,
            other => {
                return Err(AppError::InvalidInput(format!("unknown tool `{other}`")));
            }
        };
        Ok(json!({ "content": [{ "type": "text", "text": text }] }))
    }

    fn summary(&self) -> Result<String> {
        let counts = self.db.counts(None)?;
        Ok(serde_json::to_string_pretty(&json!({
            "users": counts.users,
            "posts": counts.posts,
            "networks": counts.networks.iter().map(|(s, n)| json!({ "network": s, "rows": n })).collect::<Vec<_>>(),
            "sources": counts.sources.iter().map(|(s, n)| json!({ "source": s, "rows": n })).collect::<Vec<_>>(),
            "note": "Rows are what the Twister app saw a site load. To see more of a list, queue a `scan` job for its page on its network."
        }))?)
    }

    /// The `network` argument, X when absent.
    fn network_arg(args: &Value) -> Result<Network> {
        match args.get("network").and_then(Value::as_str) {
            None => Ok(Network::default()),
            Some(slug) => Network::parse(slug).ok_or_else(|| {
                AppError::InvalidInput(format!(
                    "unknown network `{slug}`; one of {}",
                    network::ALL
                        .iter()
                        .map(|n| n.slug())
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            }),
        }
    }

    fn export(&self, args: &Value) -> Result<String> {
        let what = args.get("what").and_then(Value::as_str).unwrap_or("posts");
        let format = Format::parse(args.get("format").and_then(Value::as_str).unwrap_or("csv"))?;
        let path = args.get("path").and_then(Value::as_str).ok_or_else(|| {
            AppError::InvalidInput("`path` is required: an absolute file path to write.".into())
        })?;
        if !std::path::Path::new(path).is_absolute() {
            return Err(AppError::InvalidInput("`path` must be absolute.".into()));
        }
        let filter = args.get("filter").cloned().unwrap_or(json!({}));
        // An export is everything the filter matches unless it says otherwise.
        let (contents, rows) = match what {
            "people" | "users" => {
                let mut filter: UserFilter = serde_json::from_value(filter)?;
                filter.limit = filter.limit.or(Some(db::MAX_ROWS));
                let users = self.db.users(&filter)?;
                (export::render_users(&users, format)?, users.len())
            }
            "posts" | "bookmarks" => {
                let mut filter: PostFilter = serde_json::from_value(filter)?;
                filter.limit = filter.limit.or(Some(db::MAX_ROWS));
                if what == "bookmarks" {
                    filter.source = Some("Bookmarks".into());
                }
                let posts = self.db.posts(&filter)?;
                (export::render_posts(&posts, format)?, posts.len())
            }
            other => {
                return Err(AppError::InvalidInput(format!("unknown export `{other}`")));
            }
        };
        std::fs::write(path, contents)?;
        Ok(format!("Wrote {rows} rows to {path}"))
    }

    fn queue(&self, args: &Value) -> Result<String> {
        let network = Self::network_arg(args)?;
        let kind = args.get("kind").and_then(Value::as_str).unwrap_or("");
        let params = args.get("params").cloned().unwrap_or(json!({}));
        let dry_run = args.get("dryRun").and_then(Value::as_bool).unwrap_or(true);
        // Posting is `schedule_post`'s: a queued compose has no dry run, and
        // the tool's own schema does not offer it.
        if kind == "compose" {
            return Err(AppError::InvalidInput(
                "queue_job does not post. Use schedule_post.".into(),
            ));
        }
        ops::validate(network, kind, &params)?;
        let job =
            self.db
                .create_job(network, kind, &params.to_string(), dry_run, "mcp", "queued")?;
        Ok(format!(
            "Queued job {} ({kind} on {}{}). It runs inside the Twister app, which must be open \
             and signed in there; check list_jobs for the outcome.",
            job.id,
            network.name(),
            if dry_run { ", dry run" } else { "" }
        ))
    }

    fn schedule(&self, args: &Value) -> Result<String> {
        let network = Self::network_arg(args)?;
        let markdown = args
            .get("markdown")
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::InvalidInput("`markdown` is required.".into()))?;
        let when = args
            .get("scheduledAt")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                AppError::InvalidInput("`scheduledAt` (RFC 3339) is required.".into())
            })?;
        let post = scheduler::schedule(&self.db, network, markdown, when)?;
        Ok(format!(
            "Scheduled post {} on {} for {} as {} part(s). It goes out only while the Twister \
             app is open and signed in there; more than 15 minutes late and it is marked missed \
             instead.",
            post.id,
            network.name(),
            post.scheduled_at,
            post.parts.len()
        ))
    }
}

fn tool(name: &str, description: &str, schema: Value) -> Value {
    json!({ "name": name, "description": description, "inputSchema": schema })
}

// One table of tools, each with its schema beside it.
#[allow(clippy::too_many_lines)]
fn tools() -> Vec<Value> {
    let network = json!({
        "type": "string",
        "enum": network::ALL.iter().map(|n| n.slug()).collect::<Vec<_>>(),
        "description": "Which network. Defaults to x."
    });
    let user_filter = json!({
        "type": "object",
        "properties": {
            "network": network,
            "search": { "type": "string", "description": "Substring of handle, name or bio." },
            "source": { "type": "string", "description": "The operation that loaded them: X's Following, Followers, ListMembers; Bluesky's app.bsky.graph.getFollows…" },
            "followsMe": { "type": "boolean" },
            "followedByMe": { "type": "boolean" },
            "verified": { "type": "boolean" },
            "minFollowers": { "type": "integer" },
            "maxFollowers": { "type": "integer" },
            "minPosts": { "type": "integer" },
            "sort": { "type": "string", "enum": ["seen", "followers", "handle"] },
            "limit": { "type": "integer" }
        }
    });
    let post_filter = json!({
        "type": "object",
        "properties": {
            "network": network,
            "search": { "type": "string" },
            "source": { "type": "string", "description": "X: Bookmarks, UserTweets, HomeTimeline… Bluesky: app.bsky.feed.getAuthorFeed… Meta: the query name." },
            "kind": { "type": "string", "enum": ["post", "reply", "repost", "quote"] },
            "author": { "type": "string" },
            "bookmarked": { "type": "boolean" },
            "hasMedia": { "type": "boolean" },
            "since": { "type": "string", "description": "RFC 3339" },
            "until": { "type": "string", "description": "RFC 3339" },
            "sort": { "type": "string", "enum": ["seen", "created", "likes"] },
            "limit": { "type": "integer" }
        }
    });
    vec![
        tool(
            "store_summary",
            "How many people and posts the app has captured, by network and by source.",
            json!({ "type": "object", "properties": {} }),
        ),
        tool(
            "search_people",
            "People the app has seen, filtered.",
            user_filter.clone(),
        ),
        tool(
            "list_posts",
            "Posts the app has seen, filtered. Bookmarks are posts with source Bookmarks.",
            post_filter.clone(),
        ),
        tool(
            "export",
            "Write people or posts to a file.",
            json!({
                "type": "object",
                "properties": {
                    "what": { "type": "string", "enum": ["people", "posts", "bookmarks"] },
                    "format": { "type": "string", "enum": ["csv", "json", "markdown"] },
                    "path": { "type": "string", "description": "Absolute path to write." },
                    "filter": { "type": "object", "description": "A people or posts filter." }
                },
                "required": ["what", "path"]
            }),
        ),
        tool(
            "queue_job",
            "Queue an operation for the app to run in its signed-in page on one network: scan (scroll a page to the end, capturing what loads; params.page such as /i/bookmarks or /handle/following on X, /saved or /profile/handle/follows on Bluesky, /@handle on Threads, /handle/ on Instagram), follow or unfollow (params.handles and params.page, the list page holding them), delete (params.ids and params.page, your profile). Threads and Instagram take scans only. Dry run unless dryRun is false.",
            json!({
                "type": "object",
                "properties": {
                    "network": network,
                    "kind": { "type": "string", "enum": ["scan", "follow", "unfollow", "delete"] },
                    "params": { "type": "object" },
                    "dryRun": { "type": "boolean", "default": true }
                },
                "required": ["kind", "params"]
            }),
        ),
        tool(
            "list_jobs",
            "Recent jobs and their outcomes.",
            json!({ "type": "object", "properties": {} }),
        ),
        tool(
            "schedule_post",
            "Schedule a post on X or Bluesky, or a thread of up to 25 on X, for a time in the future. Markdown: **bold**, *italic*, `code`, lists, and --- for a thread break; long text is split at the network's limit (280 weighted on X, 300 graphemes on Bluesky). Bluesky takes one post at a time, so text that would split there is refused.",
            json!({
                "type": "object",
                "properties": {
                    "network": network,
                    "markdown": { "type": "string" },
                    "scheduledAt": { "type": "string", "description": "RFC 3339 with offset." }
                },
                "required": ["markdown", "scheduledAt"]
            }),
        ),
        tool(
            "list_scheduled_posts",
            "The schedule and what happened to each post.",
            json!({ "type": "object", "properties": {} }),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> Session {
        Session::new(Arc::new(Db::open_in_memory().expect("db")))
    }

    fn call(session: &Session, name: &str, args: Value) -> Value {
        session
            .handle(&json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call",
                "params": { "name": name, "arguments": args } }))
            .expect("answer")
    }

    fn text_of(frame: &Value) -> String {
        frame["result"]["content"][0]["text"]
            .as_str()
            .unwrap_or_default()
            .to_string()
    }

    #[test]
    fn both_eras_are_served() {
        let session = session();
        // A handshake-era client gets the revision it asked for, or the newest.
        let asked = session
            .handle(&json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize",
                "params": { "protocolVersion": "2025-06-18" } }))
            .expect("answer");
        assert_eq!(asked["result"]["protocolVersion"], "2025-06-18");
        let unknown = session
            .handle(&json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize",
                "params": { "protocolVersion": "1999-01-01" } }))
            .expect("answer");
        assert_eq!(unknown["result"]["protocolVersion"], LEGACY[0]);

        // A modern client discovers, then calls with its version in _meta.
        let discovered = session
            .handle(
                &json!({ "jsonrpc": "2.0", "id": 2, "method": "server/discover",
                "params": { "_meta": { META_VERSION: MODERN } } }),
            )
            .expect("answer");
        assert_eq!(discovered["result"]["supportedVersions"][0], MODERN);
        assert_eq!(discovered["result"]["resultType"], "complete");
        assert_eq!(
            discovered["result"]["_meta"][META_SERVER]["name"],
            "twister"
        );
        let listed = session
            .handle(&json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/list",
                "params": { "_meta": { META_VERSION: MODERN } } }))
            .expect("answer");
        assert_eq!(listed["result"]["cacheScope"], "public");
        assert!(listed["result"]["ttlMs"].as_u64().is_some());
        let refused = session
            .handle(&json!({ "jsonrpc": "2.0", "id": 4, "method": "tools/list",
                "params": { "_meta": { META_VERSION: "1900-01-01" } } }))
            .expect("answer");
        assert_eq!(refused["error"]["code"], -32022);
        assert_eq!(refused["error"]["data"]["requested"], "1900-01-01");
        assert_eq!(refused["error"]["data"]["supported"][0], MODERN);

        // An unknown tool is a protocol error, not a tool result.
        let tool = call(&session, "nope", json!({}));
        assert_eq!(tool["error"]["code"], -32602);
        // A response from the client is not answered.
        assert!(
            session
                .handle(&json!({ "jsonrpc": "2.0", "id": 9, "result": {} }))
                .is_none()
        );
        assert_eq!(parse_error()["error"]["code"], -32700);
        assert!(parse_error()["id"].is_null());
        assert!(
            session
                .handle(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))
                .is_none()
        );
        let unknown = session
            .handle(&json!({ "jsonrpc": "2.0", "id": 2, "method": "nope" }))
            .expect("answer");
        assert_eq!(unknown["error"]["code"], -32601);
    }

    #[test]
    fn every_advertised_tool_answers() {
        let session = session();
        let listed = session
            .handle(&json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }))
            .expect("answer");
        let names: Vec<String> = listed["result"]["tools"]
            .as_array()
            .expect("tools")
            .iter()
            .map(|t| t["name"].as_str().unwrap_or_default().to_string())
            .collect();
        assert_eq!(names.len(), 8);
        for name in names {
            let dir = std::env::temp_dir().join(format!("twister-mcp-{}", std::process::id()));
            let args = match name.as_str() {
                "export" => json!({ "what": "posts", "path": dir.with_extension("csv") }),
                "queue_job" => json!({ "kind": "scan", "params": { "page": "/i/bookmarks" } }),
                "schedule_post" => {
                    json!({ "markdown": "hi", "scheduledAt": "2030-01-01T09:00:00+02:00" })
                }
                _ => json!({}),
            };
            let answer = call(&session, &name, args);
            assert!(
                answer["result"]["isError"].is_null(),
                "{name}: {}",
                text_of(&answer)
            );
        }
    }

    #[test]
    fn queued_jobs_default_to_dry_runs_and_bad_ones_are_refused() {
        let session = session();
        let ok = call(
            &session,
            "queue_job",
            json!({ "kind": "unfollow", "params": { "handles": ["a"], "page": "/me/following" } }),
        );
        assert!(text_of(&ok).contains("dry run"));
        // Posting is schedule_post's; a queued post would go out for real.
        let post = call(
            &session,
            "queue_job",
            json!({ "kind": "compose", "params": { "parts": ["hi"] } }),
        );
        assert_eq!(post["result"]["isError"], true);
        let too_late = call(
            &session,
            "schedule_post",
            json!({ "markdown": "hi", "scheduledAt": "2020-01-01T09:00:00Z" }),
        );
        assert_eq!(too_late["result"]["isError"], true);
        assert!(session.db.scheduled_posts().expect("lists").is_empty());
        let bad = call(
            &session,
            "queue_job",
            json!({ "kind": "unfollow", "params": { "handles": [] } }),
        );
        assert_eq!(bad["result"]["isError"], true);
        let jobs = session.db.jobs(10).expect("jobs");
        assert_eq!(jobs.len(), 1);
        assert!(jobs[0].dry_run);
        assert_eq!(jobs[0].origin, "mcp");
        assert_eq!(jobs[0].network, Network::X);
        let bluesky = call(
            &session,
            "queue_job",
            json!({ "network": "bluesky", "kind": "scan", "params": { "page": "/saved" } }),
        );
        assert!(text_of(&bluesky).contains("on Bluesky"));
        let refused = call(
            &session,
            "queue_job",
            json!({ "network": "threads", "kind": "follow", "params": { "handles": ["zuck"], "page": "/@zuck" } }),
        );
        assert_eq!(refused["result"]["isError"], true);
        let unknown = call(
            &session,
            "queue_job",
            json!({ "network": "myspace", "kind": "scan", "params": {} }),
        );
        assert_eq!(unknown["result"]["isError"], true);
    }

    #[test]
    fn an_export_is_everything_the_filter_matches() {
        let session = session();
        let posts: Vec<crate::db::Post> = (1..=600)
            .map(|id| crate::db::Post {
                id: id.to_string(),
                author_handle: "alice".into(),
                text: "hello".into(),
                ..crate::db::Post::default()
            })
            .collect();
        session.db.record_posts(&posts).expect("records");
        let path =
            std::env::temp_dir().join(format!("twister-mcp-export-{}.json", std::process::id()));
        let all = call(
            &session,
            "export",
            json!({ "what": "posts", "format": "json", "path": path }),
        );
        assert!(
            text_of(&all).contains("Wrote 600 rows"),
            "{}",
            text_of(&all)
        );
        let some = call(
            &session,
            "export",
            json!({ "what": "posts", "format": "json", "path": path, "filter": { "limit": 10 } }),
        );
        assert!(text_of(&some).contains("Wrote 10 rows"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn scheduling_normalises_to_utc_and_refuses_empty_text() {
        let session = session();
        let ok = call(
            &session,
            "schedule_post",
            json!({ "markdown": "one\n---\ntwo", "scheduledAt": "2030-01-01T09:00:00+02:00" }),
        );
        assert!(text_of(&ok).contains("2030-01-01T07:00:00Z"));
        assert!(text_of(&ok).contains("2 part(s)"));
        let bluesky = call(
            &session,
            "schedule_post",
            json!({ "network": "bluesky", "markdown": "hi", "scheduledAt": "2030-01-01T09:00:00Z" }),
        );
        assert!(text_of(&bluesky).contains("on Bluesky"));
        let threads = call(
            &session,
            "schedule_post",
            json!({ "network": "threads", "markdown": "hi", "scheduledAt": "2030-01-01T09:00:00Z" }),
        );
        assert_eq!(threads["result"]["isError"], true);
        let empty = call(
            &session,
            "schedule_post",
            json!({ "markdown": "  ", "scheduledAt": "2030-01-01T09:00:00Z" }),
        );
        assert_eq!(empty["result"]["isError"], true);
        let relative = call(
            &session,
            "export",
            json!({ "what": "people", "path": "out.csv" }),
        );
        assert_eq!(relative["result"]["isError"], true);
    }
}
