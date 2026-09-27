//! The clock: a worker thread that publishes scheduled posts when their time
//! comes and picks up jobs the MCP binary queued.
//!
//! A desktop app only runs while it runs. A post due while Twister was
//! closed is marked `missed` once it is more than the grace window late,
//! rather than going out hours after the fact; inside the window it simply
//! goes. Publishing is the compose operation in `ops.rs`, run against the
//! signed-in session in a tab of the post's network — so it needs the app
//! open and that account signed in, and says so when it is not.

use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::time::Duration;

use chrono::Utc;
use serde_json::json;
use tauri::{AppHandle, Emitter, EventTarget, Manager};

use crate::db::{Db, ScheduledPost};
use crate::error::{AppError, Result};
use crate::network::Network;
use crate::site::{self, SHELL_LABEL};
use crate::{compose, ops, scheduled_format};

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
        if site::handle_on(app, post.network).is_none() {
            db.settle_post(
                post.id,
                "failed",
                &format!(
                    "Not signed in to {} when this was due.",
                    post.network.name()
                ),
            )?;
            changed(app);
            return Ok(());
        }
        changed(app);
        let params = json!({ "parts": post.parts });
        if let Err(err) = ops::start(
            app,
            post.network,
            "compose",
            params,
            false,
            "schedule",
            Some(post.id),
        ) {
            db.settle_post(post.id, "failed", err.message())?;
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

/// Writes a post down for later — the Write panel's and the MCP tool's one
/// way in. It is checked now by the rule it will be posted under, rather
/// than refused at its time with nobody watching.
pub fn schedule(
    db: &Db,
    network: Network,
    markdown: &str,
    scheduled_at: &str,
) -> Result<ScheduledPost> {
    let when = chrono::DateTime::parse_from_rfc3339(scheduled_at)
        .map_err(|e| AppError::InvalidInput(format!("That is not an RFC 3339 time: {e}")))?;
    if when < Utc::now() {
        return Err(AppError::InvalidInput("That time has passed.".into()));
    }
    let parts: Vec<String> = compose::prepare(network, markdown)?
        .parts
        .into_iter()
        .map(|p| p.text)
        .collect();
    if parts.is_empty() {
        return Err(AppError::InvalidInput("Nothing to post.".into()));
    }
    ops::validate(network, "compose", &json!({ "parts": parts }))?;
    db.schedule_post(network, &parts, &scheduled_format(when.with_timezone(&Utc)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_post_is_scheduled_only_by_the_rule_it_will_go_out_under() {
        let db = Db::open_in_memory().expect("db");
        let later = scheduled_format(Utc::now() + chrono::Duration::hours(1));
        let earlier = scheduled_format(Utc::now() - chrono::Duration::minutes(5));

        let post = schedule(&db, Network::X, "one\n\n---\n\ntwo", &later).expect("schedules");
        assert_eq!(post.parts, vec!["one", "two"]);
        assert_eq!(post.status, "scheduled");

        let too_late = schedule(&db, Network::X, "hi", &earlier).expect_err("in the past");
        assert!(too_late.to_string().contains("has passed"));
        assert!(schedule(&db, Network::X, "hi", "tomorrow").is_err());
        assert!(schedule(&db, Network::X, "   ", &later).is_err());
        let thread = schedule(&db, Network::Bluesky, "one\n\n---\n\ntwo", &later)
            .expect_err("Bluesky takes one post at a time");
        assert!(thread.to_string().contains("one post at a time"));
        assert!(schedule(&db, Network::Threads, "hi", &later).is_err());
        assert_eq!(db.scheduled_posts().expect("lists").len(), 1);
    }
}
