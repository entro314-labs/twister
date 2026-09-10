//! The clock: a worker thread that publishes scheduled posts when their time
//! comes and picks up jobs the MCP binary queued.
//!
//! A desktop app only runs while it runs. A post due while Twister was
//! closed is marked `missed` once it is more than the grace window late,
//! rather than going out hours after the fact; inside the window it simply
//! goes. Publishing is the compose operation in `ops.rs`, run against the
//! signed-in session in the site webview — so it needs the app open and the
//! account signed in, and says so when it is not.

use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::time::Duration;

use chrono::Utc;
use serde_json::json;
use tauri::{AppHandle, Emitter, EventTarget, Manager};

use crate::db::Db;
use crate::error::Result;
use crate::site::{self, SHELL_LABEL};
use crate::{ops, scheduled_format};

const TICK: Duration = Duration::from_secs(20);
/// A post is "missed" only once it is this far past due.
const GRACE: chrono::Duration = chrono::Duration::minutes(15);

/// The schedule changed. No payload: the shell refetches the list.
pub const EVENT_SCHEDULE: &str = "twister://schedule";

pub struct Scheduler {
    /// Held so the worker's channel stays open; dropping this stops the
    /// thread at its next tick.
    _wake: Sender<()>,
}

impl Scheduler {
    pub fn start(app: AppHandle) -> Self {
        let (wake, wakeups) = channel();
        std::thread::Builder::new()
            .name("twister-scheduler".into())
            .spawn(move || run(&app, &wakeups))
            .map_err(|e| log::error!("could not start the scheduler thread: {e}"))
            .ok();
        Self { _wake: wake }
    }
}

fn run(app: &AppHandle, wakeups: &Receiver<()>) {
    loop {
        if let Err(err) = pass(app) {
            log::error!("scheduler pass failed: {err}");
        }
        match wakeups.recv_timeout(TICK) {
            Ok(()) | Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

fn pass(app: &AppHandle) -> Result<()> {
    let db = app.state::<Db>();
    let now = Utc::now();
    if db.mark_missed(&scheduled_format(now - GRACE))? > 0 {
        changed(app);
    }
    if ops::is_running(app) {
        return Ok(());
    }
    if let Some(post) = db.claim_due_post(&scheduled_format(now))? {
        let signed_in = site::current_state(app).is_some_and(|s| s.handle.is_some());
        if !signed_in {
            db.settle_post(post.id, "failed", "Not signed in to X when this was due.")?;
            changed(app);
            return Ok(());
        }
        changed(app);
        let params = json!({ "parts": post.parts });
        if let Err(err) = ops::start(app, "compose", params, false, "schedule", Some(post.id)) {
            db.settle_post(post.id, "failed", &err.to_string())?;
            changed(app);
        }
        return Ok(());
    }
    if let Some(job) = db.next_queued_job()?
        && let Err(err) = ops::start_queued(app, job)
    {
        log::warn!("queued job refused: {err}");
    }
    Ok(())
}

pub fn changed(app: &AppHandle) {
    let _ = app.emit_to(EventTarget::webview(SHELL_LABEL), EVENT_SCHEDULE, ());
}
