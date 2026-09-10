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

use crate::db::{Db, PostFilter, UserFilter};
use crate::error::{AppError, Result};
use crate::export::{self, Format};
use crate::{compose, ops};

const PROTOCOL_VERSION: &str = "2026-07-28";

pub struct Session {
    db: Arc<Db>,
}

impl Session {
    pub fn new(db: Arc<Db>) -> Self {
        Self { db }
    }

    /// One frame. `None` for a notification, which must not be answered.
    pub fn handle(&self, message: &Value) -> Option<Value> {
        let id = message.get("id").cloned()?;
        let method = message.get("method").and_then(Value::as_str).unwrap_or("");
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        let outcome = match method {
            "initialize" => Ok(json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": { "tools": { "listChanged": false } },
                "serverInfo": { "name": "twister", "version": env!("CARGO_PKG_VERSION") },
                "instructions":
                    "Twister is a desktop client for X. This server reads the people and posts \
                     the app has seen X load (it never calls X's API itself) and queues \
                     operations the app runs in its signed-in page. Queued jobs run only while \
                     the Twister app is open, one at a time, and default to dry runs. Call \
                     store_summary first to learn what has been captured and from where."
            })),
            "tools/list" => Ok(json!({ "tools": tools() })),
            "tools/call" => self.call(&params),
            "ping" => Ok(json!({})),
            other => {
                return Some(json!({
                    "jsonrpc": "2.0", "id": id,
                    "error": { "code": -32601, "message": format!("unknown method `{other}`") }
                }));
            }
        };
        Some(match outcome {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err(err) => json!({
                "jsonrpc": "2.0", "id": id,
                "result": { "isError": true, "content": [{ "type": "text", "text": err.to_string() }] }
            }),
        })
    }

    fn call(&self, params: &Value) -> Result<Value> {
        let name = params.get("name").and_then(Value::as_str).unwrap_or("");
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
        let counts = self.db.counts()?;
        Ok(serde_json::to_string_pretty(&json!({
            "users": counts.users,
            "posts": counts.posts,
            "sources": counts.sources.iter().map(|(s, n)| json!({ "source": s, "rows": n })).collect::<Vec<_>>(),
            "note": "Rows are what the Twister app saw X load. To see more of a list, queue a `scan` job for its page."
        }))?)
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
        let (contents, rows) = match what {
            "people" | "users" => {
                let users = self.db.users(&serde_json::from_value(filter)?)?;
                (export::render_users(&users, format)?, users.len())
            }
            "posts" | "bookmarks" => {
                let mut filter: PostFilter = serde_json::from_value(filter)?;
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
        let kind = args.get("kind").and_then(Value::as_str).unwrap_or("");
        let params = args.get("params").cloned().unwrap_or(json!({}));
        let dry_run = args.get("dryRun").and_then(Value::as_bool).unwrap_or(true);
        ops::validate(kind, &params)?;
        let job = self
            .db
            .create_job(kind, &params.to_string(), dry_run, "mcp")?;
        Ok(format!(
            "Queued job {} ({kind}{}). It runs inside the Twister app, which must be open and \
             signed in; check list_jobs for the outcome.",
            job.id,
            if dry_run { ", dry run" } else { "" }
        ))
    }

    fn schedule(&self, args: &Value) -> Result<String> {
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
        let when = chrono::DateTime::parse_from_rfc3339(when)
            .map_err(|e| AppError::InvalidInput(format!("`scheduledAt` is not RFC 3339: {e}")))?;
        let prepared = compose::prepare(markdown);
        if prepared.parts.is_empty() {
            return Err(AppError::InvalidInput("Nothing to post.".into()));
        }
        let parts: Vec<String> = prepared.parts.into_iter().map(|p| p.text).collect();
        let post = self.db.schedule_post(
            &parts,
            &crate::scheduled_format(when.with_timezone(&chrono::Utc)),
        )?;
        Ok(format!(
            "Scheduled post {} for {} as {} part(s). It goes out only while the Twister app is \
             open and signed in; more than 15 minutes late and it is marked missed instead.",
            post.id,
            post.scheduled_at,
            parts.len()
        ))
    }
}

fn tool(name: &str, description: &str, schema: Value) -> Value {
    json!({ "name": name, "description": description, "inputSchema": schema })
}

fn tools() -> Vec<Value> {
    let user_filter = json!({
        "type": "object",
        "properties": {
            "search": { "type": "string", "description": "Substring of handle, name or bio." },
            "source": { "type": "string", "description": "The X operation that loaded them: Following, Followers, ListMembers…" },
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
            "search": { "type": "string" },
            "source": { "type": "string", "description": "Bookmarks, UserTweets, ListLatestTweetsTimeline, HomeTimeline…" },
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
            "How many people and posts the app has captured, by source.",
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
            "Queue an operation for the app to run in its signed-in page: scan (scroll a page to the end, capturing what loads; params.page such as /i/bookmarks or /handle/following), follow or unfollow (params.handles and params.page, the list page holding them), delete (params.ids and params.page, your profile). Dry run unless dryRun is false.",
            json!({
                "type": "object",
                "properties": {
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
            "Schedule a post or thread. Markdown: **bold**, *italic*, `code`, lists, and --- for a thread break; long text is split at 280.",
            json!({
                "type": "object",
                "properties": {
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
    fn initialize_and_notifications() {
        let session = session();
        let answer = session
            .handle(&json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize" }))
            .expect("answer");
        assert_eq!(answer["result"]["protocolVersion"], PROTOCOL_VERSION);
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
