//! Operations: the things Twister does TO the page rather than around it —
//! scrolling a list to the end, following or unfollowing a set of people,
//! deleting posts, publishing a thread.
//!
//! Each one runs inside the site, in that network's `ops.js`, because that
//! is where the buttons are. Rust is the ledger and the guard: it starts
//! exactly one at a time, brings a tab of the job's network to the front,
//! gets it to the right page, records progress as the page reports it, and
//! writes the outcome to the store. The job belongs to that tab from then
//! on, whichever tab is in front later: only it may report, a Stop goes to
//! it, and a reload or a close of it — and nothing else — fails the job,
//! because it takes the running script with it.
//!
//! Everything destructive defaults to a dry run, in the scripts and here.
//! A network that Twister only watches (see `Network::supports`) refuses
//! everything but a scan before a job is even written down.

use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter, EventTarget, Manager};

use crate::db::{self, Job};
use crate::error::{AppError, Result};
use crate::network::Network;
use crate::site::{self, SHELL_LABEL};

/// The running job, whenever it changes. Payload: [`Job`].
pub const EVENT_OP: &str = "twister://op";

const KINDS: &[&str] = &["scan", "follow", "unfollow", "delete", "compose"];
const MAX_HANDLES: usize = 2000;
const MAX_IDS: usize = 5000;
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
    /// The tab the job runs in, once it has one.
    tab: Option<u32>,
    /// The script is running in the page. Before that, a page load is the
    /// navigation the job asked for, not a reload that killed it.
    started: bool,
}

/// What a Stop has to do.
#[derive(Debug, PartialEq, Eq)]
enum Stop {
    /// Nothing is running.
    Idle,
    /// The script has not started; the job is settled here and never starts.
    Settle(i64),
    /// The script is running in this tab; it is asked to stop there.
    Ask { id: i64, tab: u32 },
}

/// A report that ended the running job, and what else it settles.
struct Settled {
    job: Job,
    finished: bool,
    scheduled_post: Option<i64>,
}

impl Ops {
    fn lock(&self) -> Result<MutexGuard<'_, Option<Running>>> {
        self.running
            .lock()
            .map_err(|_| AppError::Internal("Ops lock poisoned.".into()))
    }

    /// Makes a job the running one, or refuses because another is. `record`
    /// writes it to the ledger as running and is called only once the job is
    /// admitted, under the lock — so a refusal leaves nothing behind for the
    /// scheduler to pick up later, and two starts cannot both get in.
    fn admit(
        &self,
        scheduled_post: Option<i64>,
        record: impl FnOnce() -> Result<Job>,
    ) -> Result<Job> {
        let mut running = self.lock()?;
        if let Some(current) = running.as_ref() {
            return Err(AppError::InvalidInput(format!(
                "{} is still running. Stop it first.",
                label(&current.job.kind)
            )));
        }
        let job = record()?;
        *running = Some(Running {
            job: job.clone(),
            scheduled_post,
            tab: None,
            started: false,
        });
        Ok(job)
    }

    /// Whether `id` is still the running job.
    fn is_current(&self, id: i64) -> bool {
        self.lock()
            .is_ok_and(|r| r.as_ref().is_some_and(|r| r.job.id == id))
    }

    /// Ties the running job to the tab it was given.
    fn bind(&self, id: i64, tab: u32) {
        if let Ok(mut running) = self.lock()
            && let Some(current) = running.as_mut()
            && current.job.id == id
        {
            current.tab = Some(tab);
        }
    }

    /// Marks the script as started, if `id` is still the running job. A job
    /// stopped while its tab was getting to the page never starts.
    fn begin(&self, id: i64) -> bool {
        let Ok(mut running) = self.lock() else {
            return false;
        };
        match running.as_mut() {
            Some(current) if current.job.id == id => {
                current.started = true;
                true
            }
            _ => false,
        }
    }

    /// Whether a report from `tab` about job `id` is to be heard: only the
    /// running job's own tab speaks for it.
    fn hears(&self, tab: u32, id: i64) -> bool {
        self.lock().is_ok_and(|r| {
            r.as_ref()
                .is_some_and(|r| r.job.id == id && r.tab == Some(tab))
        })
    }

    fn stop(&self) -> Result<Stop> {
        Ok(match self.lock()?.as_ref() {
            None => Stop::Idle,
            Some(current) => match (current.started, current.tab) {
                (true, Some(tab)) => Stop::Ask {
                    id: current.job.id,
                    tab,
                },
                _ => Stop::Settle(current.job.id),
            },
        })
    }

    /// The running job, if `tab` losing its page kills it: a load in the
    /// job's tab once the script runs, or the tab closing at any point.
    fn lost(&self, tab: u32, closed: bool) -> Option<i64> {
        self.lock().ok().and_then(|r| {
            r.as_ref()
                .filter(|r| r.tab == Some(tab) && (closed || r.started))
                .map(|r| r.job.id)
        })
    }

    /// Folds a report into the running job. `None` unless it is about the
    /// running job: a stale script from a previous page cannot touch a newer
    /// one.
    fn apply(&self, id: i64, progress: Progress) -> Result<Option<Settled>> {
        let mut running = self.lock()?;
        let Some(current) = running.as_mut() else {
            return Ok(None);
        };
        if current.job.id != id {
            return Ok(None);
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
        let settled = Settled {
            job: job.clone(),
            finished,
            scheduled_post: current.scheduled_post,
        };
        if finished {
            *running = None;
        }
        Ok(Some(settled))
    }
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
pub fn validate(network: Network, kind: &str, params: &Value) -> Result<Option<String>> {
    if !KINDS.contains(&kind) {
        return Err(AppError::InvalidInput(format!(
            "Unknown operation `{kind}`."
        )));
    }
    if !network.supports(kind) {
        return Err(AppError::InvalidInput(network.refusal(kind)));
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
        return Err(AppError::InvalidInput(format!(
            "The page must be a path on {}.",
            network.name()
        )));
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
            if handles.is_empty() || !handles.iter().all(|h| network.valid_handle(h)) {
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
            if ids.is_empty() || !ids.iter().all(|id| network.valid_id(id)) {
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
            let (Some(limit), Some(most)) = (network.compose_limit(), network.max_parts()) else {
                return Err(AppError::InvalidInput(network.refusal(kind)));
            };
            let parts = strings("parts", usize::MAX)?;
            if parts.len() > most {
                return Err(AppError::InvalidInput(if most == 1 {
                    format!(
                        "Twister posts one post at a time on {}, not a thread.",
                        network.name()
                    )
                } else {
                    format!("A thread on {} is at most {most} posts.", network.name())
                }));
            }
            if parts.is_empty()
                || parts
                    .iter()
                    .any(|p| p.trim().is_empty() || !crate::compose::fits(network, p, limit))
            {
                return Err(AppError::InvalidInput(format!(
                    "Every post needs text and has to fit {}'s {limit}.",
                    network.name()
                )));
            }
        }
        _ => {}
    }
    Ok(page)
}

/// A post has no dry run: the composer sends it or it does not. A job the
/// ledger calls a dry run must not act, so a compose one is refused.
fn refuse_dry_post(kind: &str, dry_run: bool) -> Result<()> {
    if kind == "compose" && dry_run {
        return Err(AppError::InvalidInput(
            "A post has no dry run. Schedule it, or post it for real.".into(),
        ));
    }
    Ok(())
}

pub fn state(app: &AppHandle) -> Result<OpsState> {
    let running = ops(app).lock()?.as_ref().map(|r| r.job.clone());
    let recent = store(app).jobs(20)?;
    Ok(OpsState { running, recent })
}

/// Starts a job. Refuses while another runs: two scripts fighting over one
/// page is how accounts get flagged. A refused job is never written down.
pub fn start(
    app: &AppHandle,
    network: Network,
    kind: &str,
    params: Value,
    dry_run: bool,
    origin: &str,
    scheduled_post: Option<i64>,
) -> Result<Job> {
    let page = validate(network, kind, &params)?;
    refuse_dry_post(kind, dry_run)?;
    let job = ops(app).admit(scheduled_post, || {
        store(app).create_job(
            network,
            kind,
            &params.to_string(),
            dry_run,
            origin,
            "running",
        )
    })?;
    launch(app, job.clone(), page);
    Ok(job)
}

/// Runs a job that already exists in the ledger — one the MCP binary queued.
/// One that cannot start yet because another is running stays queued.
pub fn start_queued(app: &AppHandle, job: Job) -> Result<Job> {
    let params: Value = serde_json::from_str(&job.params).unwrap_or(Value::Null);
    let checked = validate(job.network, &job.kind, &params)
        .and_then(|page| refuse_dry_post(&job.kind, job.dry_run).map(|()| page));
    match checked {
        Ok(page) => {
            let job = ops(app).admit(None, || {
                let job = Job {
                    status: "running".into(),
                    ..job
                };
                store(app).update_job(&job)?;
                Ok(job)
            })?;
            launch(app, job.clone(), page);
            Ok(job)
        }
        Err(err) => {
            let failed = Job {
                status: "failed".into(),
                message: err.message().into(),
                finished_at: db::now(),
                ..job
            };
            store(app).update_job(&failed)?;
            Err(err)
        }
    }
}

/// The job is admitted and in the ledger; tells the shell and gets it going.
/// The site may need to get somewhere first, which takes a page load; the
/// wait happens off the main thread and the script starts once the page is
/// there.
fn launch(app: &AppHandle, job: Job, page: Option<String>) {
    emit(app, &job);
    let app = app.clone();
    std::thread::spawn(move || {
        if let Err(err) = reach(&app, &job, page.as_deref()).and_then(|tab| run(&app, &job, tab)) {
            log::warn!("operation {} could not start: {err}", job.id);
            let _ = report(
                &app,
                job.id,
                Progress {
                    status: Some("failed".into()),
                    message: Some(err.message().into()),
                    ..Progress::default()
                },
            );
        }
    });
}

/// Brings a tab of the job's network to the front, ties the job to it, gets
/// it onto `page` and waits for the load to settle. X redirects some paths
/// (`/i/bookmarks` lands on `/i/history`), so a page that settled somewhere
/// else after the navigation counts as reached too. Returns the tab.
fn reach(app: &AppHandle, job: &Job, page: Option<&str>) -> Result<u32> {
    let network = job.network;
    let tab = match site::active_id(app) {
        Some(id) if site::active_network(app) == Some(network) => id,
        // A tab just opened is still loading; the wait below covers it.
        _ => site::activate_network(app, network)?,
    };
    ops(app).bind(job.id, tab);
    let Some(page) = page else {
        return Ok(tab);
    };
    let gone = || AppError::NotFound("The tab closed before the page was reached.".into());
    let current_path = || {
        site::tab_snapshot(app, tab)
            .and_then(|state| url::Url::parse(&state.url).ok())
            .map(|url| url.path().trim_end_matches('/').to_string())
            .unwrap_or_default()
    };
    let wanted = page.trim_end_matches('/').to_string();
    let before = current_path();
    if before == wanted {
        return Ok(tab);
    }
    // In-app first: a full load of an X route sits on the splash screen for
    // a long while, and every site's router takes a pushState the way it
    // takes Back.
    let in_app = site::go_path(app, tab, page).is_ok();
    let quick = Instant::now() + Duration::from_secs(3);
    while in_app && Instant::now() < quick && current_path() != wanted {
        std::thread::sleep(Duration::from_millis(100));
    }
    if current_path() != wanted {
        site::navigate(app, tab, &format!("{}{page}", network.origin()))?;
    }
    let deadline = Instant::now() + NAVIGATION_TIMEOUT;
    let mut stable_since: Option<(String, Instant)> = None;
    loop {
        // Stopped while getting there: `run` will not start it.
        if !ops(app).is_current(job.id) {
            return Ok(tab);
        }
        let loading = site::tab_snapshot(app, tab).ok_or_else(gone)?.loading;
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
    Ok(tab)
}

/// Starts the script in the job's tab — unless the job was stopped or
/// settled while its tab was getting to the page.
fn run(app: &AppHandle, job: &Job, tab: u32) -> Result<()> {
    if !ops(app).begin(job.id) {
        return Ok(());
    }
    let params: Value = serde_json::from_str(&job.params)?;
    let kind = serde_json::to_string(&job.kind)?;
    let dry_run = if job.dry_run { "true" } else { "false" };
    site::eval(
        app,
        tab,
        &format!(
            "window.__twisterOps ? window.__twisterOps.run({}, {kind}, {params}, {dry_run}) : \
             window.__TAURI_INTERNALS__.invoke('site_op_progress', {{ id: {}, progress: \
             {{ status: 'failed', message: 'The operations script is not on this page.' }} }})",
            job.id, job.id
        ),
    )
}

/// A report from a page. Only the running job's own tab may speak, and only
/// about that job — whichever tab is in front; posts it says it deleted
/// leave the store only for a live delete run — anything on the site can
/// call the command, so nothing in it is taken on trust.
pub fn report_from(
    app: &AppHandle,
    caller: &str,
    id: i64,
    progress: Progress,
    removed: &[String],
) -> Result<()> {
    let Some(tab) = site::id_from_label(caller) else {
        return Ok(());
    };
    if !ops(app).hears(tab, id) {
        return Ok(());
    }
    let live_delete = ops(app)
        .lock()?
        .as_ref()
        .filter(|r| r.job.id == id && r.job.kind == "delete" && !r.job.dry_run)
        .map(|r| r.job.network);
    if let Some(network) = live_delete
        && !removed.is_empty()
    {
        let ids: Vec<String> = removed
            .iter()
            .filter(|id| network.valid_id(id))
            .take(500)
            .cloned()
            .collect();
        store(app).remove_posts(network, &ids)?;
    }
    report(app, id, progress)
}

/// A report about the running job, from its page or from Rust. Ignored
/// unless it is about the running job.
pub fn report(app: &AppHandle, id: i64, progress: Progress) -> Result<()> {
    let Some(Settled {
        job,
        finished,
        scheduled_post,
    }) = ops(app).apply(id, progress)?
    else {
        return Ok(());
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
    let cancelled = |id| {
        report(
            app,
            id,
            Progress {
                status: Some("cancelled".into()),
                ..Progress::default()
            },
        )
    };
    match ops(app).stop()? {
        Stop::Idle => Ok(()),
        // Still getting to its page: it never starts.
        Stop::Settle(id) => cancelled(id),
        // The script stops at its next step and reports `cancelled`; if its
        // tab is gone, it is settled here.
        Stop::Ask { id, tab } => {
            if site::eval(
                app,
                tab,
                "window.__twisterOps && window.__twisterOps.cancel()",
            )
            .is_err()
            {
                cancelled(id)?;
            }
            Ok(())
        }
    }
}

/// A full page load in the job's tab takes the running script with it. A
/// load before the script started is the job's own navigation and is left
/// alone; loads in other tabs are none of the job's business.
pub fn page_reloaded(app: &AppHandle, tab: u32) {
    settle_lost(app, tab, false, "The page reloaded while this was running.");
}

/// The job's tab is closing, and the job with it, whatever stage it is at.
pub fn tab_closed(app: &AppHandle, tab: u32) {
    settle_lost(app, tab, true, "The tab closed while this was running.");
}

fn settle_lost(app: &AppHandle, tab: u32, closed: bool, message: &str) {
    if let Some(id) = ops(app).lost(tab, closed) {
        let _ = report(
            app,
            id,
            Progress {
                status: Some("failed".into()),
                message: Some(message.into()),
                ..Progress::default()
            },
        );
    }
}

pub fn is_running(app: &AppHandle) -> bool {
    ops(app).lock().map_or(true, |r| r.is_some())
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
        let x = Network::X;
        assert!(validate(x, "scan", &json!({})).expect("ok").is_none());
        assert_eq!(
            validate(x, "scan", &json!({ "page": "/i/bookmarks" })).expect("ok"),
            Some("/i/bookmarks".into())
        );
        assert!(validate(x, "scan", &json!({ "page": "https://x.com/home" })).is_err());
        assert!(validate(x, "scan", &json!({ "page": "/a//b" })).is_err());
        assert!(validate(x, "scan", &json!([])).is_err());
        assert!(validate(x, "dance", &json!({})).is_err());

        assert!(
            validate(
                x,
                "unfollow",
                &json!({ "handles": ["a"], "page": "/me/following" })
            )
            .is_ok()
        );
        assert!(validate(x, "unfollow", &json!({ "handles": ["a"] })).is_err());
        assert!(validate(x, "unfollow", &json!({ "handles": [], "page": "/x" })).is_err());
        assert!(
            validate(
                x,
                "follow",
                &json!({ "handles": ["not a handle"], "page": "/x" })
            )
            .is_err()
        );
        assert!(validate(x, "follow", &json!({ "handles": [1], "page": "/x" })).is_err());

        assert!(validate(x, "delete", &json!({ "ids": ["1"], "page": "/me" })).is_ok());
        assert!(validate(x, "delete", &json!({ "ids": ["x"], "page": "/me" })).is_err());

        assert!(validate(x, "compose", &json!({ "parts": ["hello"] })).is_ok());
        assert!(validate(x, "compose", &json!({ "parts": [""] })).is_err());
        assert!(validate(x, "compose", &json!({ "parts": [] })).is_err());
        // Each part has to fit the network's own count, not a byte cap.
        assert!(validate(x, "compose", &json!({ "parts": ["x".repeat(280)] })).is_ok());
        assert!(validate(x, "compose", &json!({ "parts": ["x".repeat(281)] })).is_err());
        assert!(validate(x, "compose", &json!({ "parts": vec!["hi"; 25] })).is_ok());
        assert!(validate(x, "compose", &json!({ "parts": vec!["hi"; 26] })).is_err());
    }

    #[test]
    fn a_post_is_never_a_dry_run() {
        assert!(refuse_dry_post("compose", true).is_err());
        assert!(refuse_dry_post("compose", false).is_ok());
        assert!(refuse_dry_post("delete", true).is_ok());
    }

    #[test]
    fn bluesky_takes_one_post_at_a_time_and_its_own_count() {
        let bluesky = Network::Bluesky;
        assert!(validate(bluesky, "compose", &json!({ "parts": ["x".repeat(300)] })).is_ok());
        assert!(validate(bluesky, "compose", &json!({ "parts": ["x".repeat(301)] })).is_err());
        // 300 graphemes, but past the record's 3000 bytes.
        let family = "👨‍👩‍👧‍👦".repeat(150);
        assert!(family.len() > 3000);
        assert!(validate(bluesky, "compose", &json!({ "parts": [family] })).is_err());
        let thread = validate(bluesky, "compose", &json!({ "parts": ["one", "two"] }))
            .expect_err("a thread is refused");
        assert!(thread.to_string().contains("one post at a time"));
    }

    fn admitted(ops: &Ops, db: &db::Db, kind: &str) -> Job {
        ops.admit(None, || {
            db.create_job(Network::X, kind, "{}", false, "app", "running")
        })
        .expect("admitted")
    }

    #[test]
    fn a_refused_start_leaves_nothing_behind_to_run_later() {
        let db = db::Db::open_in_memory().expect("db");
        let ops = Ops::default();
        let first = admitted(&ops, &db, "unfollow");
        let mut recorded = false;
        let refused = ops
            .admit(None, || {
                recorded = true;
                db.create_job(Network::X, "delete", "{}", false, "app", "running")
            })
            .expect_err("one at a time");
        assert!(refused.to_string().contains("Unfollow is still running"));
        assert!(!recorded, "a refused job is never written down");
        assert!(db.next_queued_job().expect("reads").is_none());
        let jobs = db.jobs(10).expect("lists");
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].id, first.id);
        assert_eq!(jobs[0].status, "running");
    }

    #[test]
    fn only_the_jobs_own_tab_is_heard_whichever_tab_is_in_front() {
        let db = db::Db::open_in_memory().expect("db");
        let ops = Ops::default();
        let job = admitted(&ops, &db, "scan");
        // Nothing is heard before the job has a tab.
        assert!(!ops.hears(3, job.id));
        ops.bind(job.id, 3);
        assert!(ops.begin(job.id));
        assert!(ops.hears(3, job.id));
        assert!(!ops.hears(4, job.id), "another tab cannot speak for it");
        assert!(
            !ops.hears(3, job.id + 1),
            "nor can its tab speak for another job"
        );
    }

    #[test]
    fn a_stop_before_the_script_starts_settles_it_and_it_never_starts() {
        let db = db::Db::open_in_memory().expect("db");
        let ops = Ops::default();
        let job = admitted(&ops, &db, "delete");
        ops.bind(job.id, 3);
        assert_eq!(ops.stop().expect("stops"), Stop::Settle(job.id));
        let settled = ops
            .apply(
                job.id,
                Progress {
                    status: Some("cancelled".into()),
                    ..Progress::default()
                },
            )
            .expect("applies")
            .expect("about the running job");
        assert!(settled.finished);
        assert_eq!(settled.job.status, "cancelled");
        assert!(
            !ops.begin(job.id),
            "the navigation finishing must not start it"
        );
        assert_eq!(ops.stop().expect("stops"), Stop::Idle);

        let next = admitted(&ops, &db, "delete");
        ops.bind(next.id, 5);
        assert!(ops.begin(next.id));
        assert_eq!(
            ops.stop().expect("stops"),
            Stop::Ask {
                id: next.id,
                tab: 5
            }
        );
    }

    #[test]
    fn only_the_jobs_tab_losing_its_page_fails_it() {
        let db = db::Db::open_in_memory().expect("db");
        let ops = Ops::default();
        let job = admitted(&ops, &db, "follow");
        ops.bind(job.id, 3);
        // The job's own navigation, before the script runs, is not a loss.
        assert_eq!(ops.lost(3, false), None);
        // Closing its tab is, at any stage.
        assert_eq!(ops.lost(3, true), Some(job.id));
        assert!(ops.begin(job.id));
        assert_eq!(ops.lost(3, false), Some(job.id));
        // A new tab loading, or another one closing, is not.
        assert_eq!(ops.lost(4, false), None);
        assert_eq!(ops.lost(4, true), None);
    }

    #[test]
    fn reports_about_another_job_change_nothing() {
        let db = db::Db::open_in_memory().expect("db");
        let ops = Ops::default();
        let job = admitted(&ops, &db, "scan");
        let stale = Progress {
            status: Some("done".into()),
            ..Progress::default()
        };
        assert!(ops.apply(job.id + 1, stale).expect("applies").is_none());
        assert!(ops.is_current(job.id));
        let progress = ops
            .apply(
                job.id,
                Progress {
                    done: Some(4),
                    total: Some(-1),
                    ..Progress::default()
                },
            )
            .expect("applies")
            .expect("heard");
        assert!(!progress.finished);
        assert_eq!(progress.job.done, 4);
        assert_eq!(progress.job.total, 0);
        assert!(ops.is_current(job.id));
    }

    #[test]
    fn each_network_has_its_own_handles_and_its_own_refusals() {
        let bluesky = Network::Bluesky;
        assert!(
            validate(
                bluesky,
                "follow",
                &json!({ "handles": ["a.bsky.social"], "page": "/profile/me/follows" })
            )
            .is_ok()
        );
        // An X handle is not a Bluesky handle.
        assert!(
            validate(
                bluesky,
                "follow",
                &json!({ "handles": ["alice"], "page": "/profile/me/follows" })
            )
            .is_err()
        );
        assert!(
            validate(
                bluesky,
                "delete",
                &json!({ "ids": ["at://did:plc:a/app.bsky.feed.post/3k"], "page": "/profile/me" })
            )
            .is_ok()
        );
        // Meta's sites are watched, never driven.
        for network in [Network::Threads, Network::Instagram] {
            assert!(validate(network, "scan", &json!({ "page": "/@zuck" })).is_ok());
            let refused = validate(
                network,
                "follow",
                &json!({ "handles": ["zuck"], "page": "/@zuck/followers" }),
            )
            .expect_err("refused");
            assert!(refused.to_string().contains("does not follow"));
            assert!(validate(network, "compose", &json!({ "parts": ["hi"] })).is_err());
            assert!(validate(network, "delete", &json!({ "ids": ["1"], "page": "/@me" })).is_err());
        }
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
