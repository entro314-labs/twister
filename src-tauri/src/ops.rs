//! Operations: the things Twister does TO the page rather than around it —
//! scrolling a list to the end, following or unfollowing a set of people,
//! deleting posts, publishing a thread.
//!
//! Each one runs inside x.com, in `site/ops.js`, because that is where the
//! buttons are. Rust is the ledger and the guard: it starts exactly one at a
//! time, gets the site to the right page first, records progress as the page
//! reports it, and writes the outcome to the store. A page that reloads
//! mid-run takes the running script with it, so a reload fails the job
//! rather than leaving it "running" forever.
//!
//! Everything destructive defaults to a dry run, in `ops.js` and here.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter, EventTarget, Manager};

use crate::capture::valid_id;
use crate::db::{self, Job};
use crate::error::{AppError, Result};
use crate::site::{self, SHELL_LABEL, valid_handle};

/// The running job, whenever it changes. Payload: [`Job`].
pub const EVENT_OP: &str = "twister://op";

const KINDS: &[&str] = &["scan", "follow", "unfollow", "delete", "compose"];
const MAX_HANDLES: usize = 2000;
const MAX_IDS: usize = 5000;
const MAX_PARTS: usize = 25;
const MAX_PART: usize = 4000;
/// How long the site gets to reach the page an operation needs.
const NAVIGATION_TIMEOUT: Duration = Duration::from_secs(25);

#[derive(Default)]
pub struct Ops {
    running: Mutex<Option<Running>>,
}

struct Running {
    job: Job,
    /// Set when the scheduler started this to publish a scheduled post.
    scheduled_post: Option<i64>,
    /// The script is running in the page. Before that, a page load is the
    /// navigation the job asked for, not a reload that killed it.
    started: bool,
}

/// What the page reports. Every field optional: a report says what changed.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Progress {
    pub total: Option<i64>,
    pub done: Option<i64>,
    pub skipped: Option<i64>,
    pub failed: Option<i64>,
    pub message: Option<String>,
    /// `running` | `done` | `failed` | `cancelled`
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpsState {
    pub running: Option<Job>,
    pub recent: Vec<Job>,
}

fn ops(app: &AppHandle) -> tauri::State<'_, Ops> {
    app.state::<Ops>()
}

fn store(app: &AppHandle) -> tauri::State<'_, db::Db> {
    app.state::<db::Db>()
}

/// Checks a job's parameters before anything runs, and returns the page the
/// site has to be on for it, if any.
pub fn validate(kind: &str, params: &Value) -> Result<Option<String>> {
    if !KINDS.contains(&kind) {
        return Err(AppError::InvalidInput(format!(
            "Unknown operation `{kind}`."
        )));
    }
    let object = params
        .as_object()
        .ok_or_else(|| AppError::InvalidInput("Operation parameters must be an object.".into()))?;
    let page = object
        .get("page")
        .and_then(Value::as_str)
        .map(str::to_string);
    if let Some(page) = &page
        && (!page.starts_with('/') || page.len() > 200 || page.contains("//"))
    {
        return Err(AppError::InvalidInput("The page must be an X path.".into()));
    }
    let strings = |key: &str, max: usize| -> Result<Vec<String>> {
        let list = object
            .get(key)
            .and_then(Value::as_array)
            .ok_or_else(|| AppError::InvalidInput(format!("`{key}` must be a list.")))?;
        if list.len() > max {
            return Err(AppError::InvalidInput(format!(
                "At most {max} {key} per run."
            )));
        }
        list.iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| AppError::InvalidInput(format!("`{key}` must be strings.")))
            })
            .collect()
    };
    match kind {
        "follow" | "unfollow" => {
            let handles = strings("handles", MAX_HANDLES)?;
            if handles.is_empty() || !handles.iter().all(|h| valid_handle(h)) {
                return Err(AppError::InvalidInput(
                    "Give at least one handle, and only handles.".into(),
                ));
            }
            if page.is_none() {
                return Err(AppError::InvalidInput(
                    "A follow operation needs the page whose list holds these people.".into(),
                ));
            }
        }
        "delete" => {
            let ids = strings("ids", MAX_IDS)?;
            if ids.is_empty() || !ids.iter().all(|id| valid_id(id)) {
                return Err(AppError::InvalidInput(
                    "Give at least one post id, and only ids.".into(),
                ));
            }
            if page.is_none() {
                return Err(AppError::InvalidInput(
                    "A delete operation needs your profile page.".into(),
                ));
            }
        }
        "compose" => {
            let parts = strings("parts", MAX_PARTS)?;
            if parts.is_empty()
                || parts
                    .iter()
                    .any(|p| p.trim().is_empty() || p.len() > MAX_PART)
            {
                return Err(AppError::InvalidInput(
                    "A post needs text, and each part has a size limit.".into(),
                ));
            }
        }
        _ => {}
    }
    Ok(page)
}

pub fn state(app: &AppHandle) -> Result<OpsState> {
    let running = ops(app)
        .running
        .lock()
        .map_err(|_| AppError::Internal("Ops lock poisoned.".into()))?
        .as_ref()
        .map(|r| r.job.clone());
    let recent = store(app).jobs(20)?;
    Ok(OpsState { running, recent })
}

/// Starts a job. Refuses while another runs: two scripts fighting over one
/// page is how accounts get flagged.
pub fn start(
    app: &AppHandle,
    kind: &str,
    params: Value,
    dry_run: bool,
    origin: &str,
    scheduled_post: Option<i64>,
) -> Result<Job> {
    let page = validate(kind, &params)?;
    let job = store(app).create_job(kind, &params.to_string(), dry_run, origin)?;
    launch(app, job, page, scheduled_post)
}

/// Runs a job that already exists in the ledger — one the MCP binary queued.
pub fn start_queued(app: &AppHandle, job: Job) -> Result<Job> {
    let params: Value = serde_json::from_str(&job.params).unwrap_or(Value::Null);
    match validate(&job.kind, &params) {
        Ok(page) => launch(app, job, page, None),
        Err(err) => {
            let failed = Job {
                status: "failed".into(),
                message: err.to_string(),
                finished_at: db::now(),
                ..job
            };
            store(app).update_job(&failed)?;
            Err(err)
        }
    }
}

fn launch(
    app: &AppHandle,
    job: Job,
    page: Option<String>,
    scheduled_post: Option<i64>,
) -> Result<Job> {
    {
        let state = ops(app);
        let mut running = state
            .running
            .lock()
            .map_err(|_| AppError::Internal("Ops lock poisoned.".into()))?;
        if let Some(current) = running.as_ref() {
            return Err(AppError::InvalidInput(format!(
                "{} is still running. Stop it first.",
                label(&current.job.kind)
            )));
        }
        let mut job = job;
        job.status = "running".into();
        store(app).update_job(&job)?;
        *running = Some(Running {
            job: job.clone(),
            scheduled_post,
            started: false,
        });
        emit(app, &job);
    }
    let job = ops(app)
        .running
        .lock()
        .map(|r| r.as_ref().map(|r| r.job.clone()))
        .ok()
        .flatten()
        .ok_or_else(|| AppError::Internal("The job vanished on start.".into()))?;

    // The site may need to get somewhere first, which takes a page load;
    // the wait happens off the main thread and the script starts once the
    // page is there.
    let app = app.clone();
    let started = job.clone();
    std::thread::spawn(move || {
        if let Err(err) = reach(&app, page.as_deref()).and_then(|()| run(&app, &started)) {
            log::warn!("operation {} could not start: {err}", started.id);
            let _ = report(
                &app,
                started.id,
                Progress {
                    status: Some("failed".into()),
                    message: Some(err.to_string()),
                    ..Progress::default()
                },
            );
        }
    });
    Ok(job)
}

/// Gets the site onto `page` and waits for the load to settle. X redirects
/// some paths (`/i/bookmarks` lands on `/i/history`), so a page that settled
/// somewhere else after the navigation counts as reached too.
fn reach(app: &AppHandle, page: Option<&str>) -> Result<()> {
    let Some(page) = page else {
        return Ok(());
    };
    let current_path = || {
        site::current_state(app)
            .and_then(|state| url::Url::parse(&state.url).ok())
            .map(|url| url.path().trim_end_matches('/').to_string())
            .unwrap_or_default()
    };
    let wanted = page.trim_end_matches('/').to_string();
    let before = current_path();
    if before == wanted {
        return Ok(());
    }
    // In-app first: a full load of an X route sits on the splash screen for
    // a long while, and X's router takes a pushState the way it takes Back.
    let in_app = site::go_path(app, page).is_ok();
    let quick = Instant::now() + Duration::from_secs(3);
    while in_app && Instant::now() < quick && current_path() != wanted {
        std::thread::sleep(Duration::from_millis(100));
    }
    if current_path() != wanted {
        site::navigate(app, &format!("https://x.com{page}"))?;
    }
    let deadline = Instant::now() + NAVIGATION_TIMEOUT;
    let mut stable_since: Option<(String, Instant)> = None;
    loop {
        let loading = site::current_state(app).is_none_or(|state| state.loading);
        let path = current_path();
        if !loading && path == wanted {
            break;
        }
        if !loading && path != before {
            match &stable_since {
                Some((seen, since)) if *seen == path => {
                    if since.elapsed() > Duration::from_millis(1500) {
                        log::debug!("{page} settled on {path}");
                        break;
                    }
                }
                _ => stable_since = Some((path, Instant::now())),
            }
        }
        if Instant::now() > deadline {
            return Err(AppError::Internal(format!(
                "The site did not reach {page} in time."
            )));
        }
        std::thread::sleep(Duration::from_millis(150));
    }
    // The document is there; give X a moment to draw the first cells.
    std::thread::sleep(Duration::from_millis(1200));
    Ok(())
}

fn run(app: &AppHandle, job: &Job) -> Result<()> {
    if let Ok(mut running) = ops(app).running.lock()
        && let Some(current) = running.as_mut()
        && current.job.id == job.id
    {
        current.started = true;
    }
    let params: Value = serde_json::from_str(&job.params)?;
    let kind = serde_json::to_string(&job.kind)?;
    let dry_run = if job.dry_run { "true" } else { "false" };
    site::eval(
        app,
        &format!(
            "window.__twisterOps ? window.__twisterOps.run({}, {kind}, {params}, {dry_run}) : \
             window.__TAURI_INTERNALS__.invoke('site_op_progress', {{ id: {}, progress: \
             {{ status: 'failed', message: 'The operations script is not on this page.' }} }})",
            job.id, job.id
        ),
    )
}

/// A report from a page. Only the active tab may speak, and only about the
/// running job; posts it says it deleted leave the store only for a live
/// delete run — anything on x.com can call the command, so nothing in it is
/// taken on trust.
pub fn report_from(
    app: &AppHandle,
    caller: &str,
    id: i64,
    progress: Progress,
    removed: &[String],
) -> Result<()> {
    if site::active_label(app).as_deref() != Some(caller) {
        return Ok(());
    }
    let live_delete = ops(app)
        .running
        .lock()
        .map_err(|_| AppError::Internal("Ops lock poisoned.".into()))?
        .as_ref()
        .is_some_and(|r| r.job.id == id && r.job.kind == "delete" && !r.job.dry_run);
    if live_delete && !removed.is_empty() {
        let ids: Vec<String> = removed
            .iter()
            .filter(|id| valid_id(id))
            .take(500)
            .cloned()
            .collect();
        store(app).remove_posts(&ids)?;
    }
    report(app, id, progress)
}

/// The page's report. Ignored unless it is about the running job: a stale
/// script from a previous page cannot touch a newer one.
pub fn report(app: &AppHandle, id: i64, progress: Progress) -> Result<()> {
    let (job, finished, scheduled_post) = {
        let state = ops(app);
        let mut running = state
            .running
            .lock()
            .map_err(|_| AppError::Internal("Ops lock poisoned.".into()))?;
        let Some(current) = running.as_mut() else {
            return Ok(());
        };
        if current.job.id != id {
            return Ok(());
        }
        let job = &mut current.job;
        if let Some(v) = progress.total {
            job.total = v.max(0);
        }
        if let Some(v) = progress.done {
            job.done = v.max(0);
        }
        if let Some(v) = progress.skipped {
            job.skipped = v.max(0);
        }
        if let Some(v) = progress.failed {
            job.failed = v.max(0);
        }
        if let Some(message) = progress.message {
            job.message = message.chars().take(500).collect();
        }
        let finished = match progress.status.as_deref() {
            Some(status @ ("done" | "failed" | "cancelled")) => {
                job.status = status.into();
                job.finished_at = db::now();
                true
            }
            _ => false,
        };
        let job = job.clone();
        let scheduled_post = current.scheduled_post;
        if finished {
            *running = None;
        }
        (job, finished, scheduled_post)
    };
    if finished {
        store(app).update_job(&job)?;
        if let Some(post_id) = scheduled_post {
            let (status, error) = if job.status == "done" {
                ("posted", String::new())
            } else {
                ("failed", job.message.clone())
            };
            store(app).settle_post(post_id, status, &error)?;
            crate::scheduler::changed(app);
        }
        site::notify(app, summary(&job));
    }
    emit(app, &job);
    Ok(())
}

pub fn cancel(app: &AppHandle) -> Result<()> {
    let id = ops(app)
        .running
        .lock()
        .map_err(|_| AppError::Internal("Ops lock poisoned.".into()))?
        .as_ref()
        .map(|r| r.job.id);
    let Some(id) = id else {
        return Ok(());
    };
    // The script stops at its next step and reports `cancelled`; if the page
    // is gone the reload handler settles it instead.
    if site::eval(app, "window.__twisterOps && window.__twisterOps.cancel()").is_err() {
        report(
            app,
            id,
            Progress {
                status: Some("cancelled".into()),
                ..Progress::default()
            },
        )?;
    }
    Ok(())
}

/// A full page load takes the running script with it. A load before the
/// script started is the job's own navigation and is left alone.
pub fn page_reloaded(app: &AppHandle) {
    let id = ops(app)
        .running
        .lock()
        .ok()
        .and_then(|r| r.as_ref().filter(|r| r.started).map(|r| r.job.id));
    if let Some(id) = id {
        let _ = report(
            app,
            id,
            Progress {
                status: Some("failed".into()),
                message: Some("The page reloaded while this was running.".into()),
                ..Progress::default()
            },
        );
    }
}

pub fn is_running(app: &AppHandle) -> bool {
    ops(app).running.lock().map_or(true, |r| r.is_some())
}

fn emit(app: &AppHandle, job: &Job) {
    let _ = app.emit_to(EventTarget::webview(SHELL_LABEL), EVENT_OP, job.clone());
}

pub fn label(kind: &str) -> &'static str {
    match kind {
        "scan" => "Scan",
        "follow" => "Follow",
        "unfollow" => "Unfollow",
        "delete" => "Delete",
        "compose" => "Post",
        _ => "Operation",
    }
}

/// One line for the status bar when a job ends.
pub fn summary(job: &Job) -> String {
    let what = label(&job.kind);
    let dry = if job.dry_run { " (dry run)" } else { "" };
    match job.status.as_str() {
        "done" => format!(
            "{what}{dry}: {} done, {} skipped, {} failed",
            job.done, job.skipped, job.failed
        ),
        "cancelled" => format!("{what}{dry} stopped after {}", job.done),
        _ => format!("{what}{dry} failed: {}", job.message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn kinds_and_parameters_are_checked() {
        assert!(validate("scan", &json!({})).expect("ok").is_none());
        assert_eq!(
            validate("scan", &json!({ "page": "/i/bookmarks" })).expect("ok"),
            Some("/i/bookmarks".into())
        );
        assert!(validate("scan", &json!({ "page": "https://x.com/home" })).is_err());
        assert!(validate("scan", &json!({ "page": "/a//b" })).is_err());
        assert!(validate("scan", &json!([])).is_err());
        assert!(validate("dance", &json!({})).is_err());

        assert!(
            validate(
                "unfollow",
                &json!({ "handles": ["a"], "page": "/me/following" })
            )
            .is_ok()
        );
        assert!(validate("unfollow", &json!({ "handles": ["a"] })).is_err());
        assert!(validate("unfollow", &json!({ "handles": [], "page": "/x" })).is_err());
        assert!(
            validate(
                "follow",
                &json!({ "handles": ["not a handle"], "page": "/x" })
            )
            .is_err()
        );
        assert!(validate("follow", &json!({ "handles": [1], "page": "/x" })).is_err());

        assert!(validate("delete", &json!({ "ids": ["1"], "page": "/me" })).is_ok());
        assert!(validate("delete", &json!({ "ids": ["x"], "page": "/me" })).is_err());

        assert!(validate("compose", &json!({ "parts": ["hello"] })).is_ok());
        assert!(validate("compose", &json!({ "parts": [""] })).is_err());
        assert!(validate("compose", &json!({ "parts": [] })).is_err());
        let long = "x".repeat(MAX_PART + 1);
        assert!(validate("compose", &json!({ "parts": [long] })).is_err());
    }

    #[test]
    fn summaries_read_as_one_line() {
        let job = Job {
            kind: "unfollow".into(),
            status: "done".into(),
            dry_run: true,
            done: 3,
            skipped: 1,
            ..Job::default()
        };
        assert_eq!(
            summary(&job),
            "Unfollow (dry run): 3 done, 1 skipped, 0 failed"
        );
        let failed = Job {
            kind: "delete".into(),
            status: "failed".into(),
            message: "toast".into(),
            ..Job::default()
        };
        assert_eq!(summary(&failed), "Delete failed: toast");
    }
}
