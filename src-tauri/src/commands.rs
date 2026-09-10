//! Every Tauri command, in one place. The shell's and the bridge's commands are
//! kept apart by the capability files, not by this module: `site_*` are the
//! ones x.com may call, and each takes the calling webview so it can only
//! ever speak for its own tab.

use std::sync::Mutex;

use serde_json::Value;
use tauri::{AppHandle, Manager, State, Webview};

use crate::db::{Counts, Db, Job, Post, PostFilter, ScheduledPost, User, UserFilter};
use crate::error::{AppError, Result, internal};
use crate::settings::{Settings, SitePrefs, Store, WindowBounds};
use crate::site::{self, Action, Destination, Insets, Site, SiteState};
use crate::tooltip::{self, Anchor, Content};
use crate::userland::{self, UserAssets};
use crate::{capture, compose, download, export, ops, scheduler, update};

pub struct AppState {
    pub store: Store,
    pub settings: Mutex<Settings>,
    pub site: Site,
    /// The window's last known placement, written to disk on close and quit.
    pub bounds: Mutex<Option<WindowBounds>>,
}

fn poisoned() -> AppError {
    AppError::Internal("A lock was poisoned.".into())
}

// ─── Shell ──────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Result<Settings> {
    state
        .settings
        .lock()
        .map(|s| s.clone())
        .map_err(|_| poisoned())
}

#[tauri::command]
pub fn update_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: Settings,
) -> Result<Settings> {
    settings.validate()?;
    state.store.save_settings(&settings)?;
    {
        let mut current = state.settings.lock().map_err(|_| poisoned())?;
        *current = settings.clone();
    }
    // The material is applied by the shell through `set_window_material`,
    // which is the only way it learns what the OS actually did.
    site::apply_prefs(&app, &settings.site_prefs())?;
    Ok(settings)
}

#[tauri::command]
pub fn get_site_state(app: AppHandle) -> SiteState {
    site::state(&app)
}

#[tauri::command]
pub fn navigate_site(app: AppHandle, destination: Destination) -> Result<()> {
    site::go(&app, destination)
}

#[tauri::command]
pub fn site_action(app: AppHandle, action: Action) -> Result<()> {
    site::act(&app, action)
}

#[tauri::command]
pub fn set_site_insets(app: AppHandle, insets: Insets) -> Result<()> {
    site::set_insets(&app, insets)
}

#[tauri::command]
pub fn set_site_visible(app: AppHandle, visible: bool) -> Result<()> {
    site::set_visible(&app, visible)
}

#[tauri::command]
pub fn sign_out(app: AppHandle) -> Result<()> {
    site::sign_out(&app)
}

/// The shell has painted its first frame. Until now the window was hidden, so
/// launch never shows an unthemed flash against a transparent window.
#[tauri::command]
pub fn shell_ready(app: AppHandle) {
    crate::show_main_window(&app);
}

#[tauri::command]
pub fn set_window_material(app: AppHandle, material: String) -> Result<String> {
    let window = app
        .get_window(crate::MAIN_WINDOW)
        .ok_or_else(|| AppError::NotFound("The main window is gone.".into()))?;
    Ok(crate::windowing::apply_material(&window, &material))
}

#[tauri::command]
pub fn list_user_assets() -> Result<UserAssets> {
    userland::list()
}

#[tauri::command]
pub fn open_user_assets_dir() -> Result<()> {
    let dir = userland::dir()?;
    tauri_plugin_opener::open_path(dir, None::<&str>)
        .map_err(|err| internal("Opening the scripts folder", err))
}

/// Rebuilds every tab so scripts and styles added to the folder start
/// running. The pages reload; that is the cost of an initialization script.
#[tauri::command]
pub fn reload_site(app: AppHandle) -> Result<()> {
    site::rebuild(&app)
}

#[tauri::command]
pub fn show_tooltip(app: AppHandle, anchor: Anchor, content: Content) -> Result<()> {
    tooltip::show(&app, anchor, content)
}

#[tauri::command]
pub fn hide_tooltip(app: AppHandle) -> Result<()> {
    tooltip::hide(&app)
}

// ─── Updates ────────────────────────────────────────────────────────────────

/// The channel this install polls right now: the preference, with `auto`
/// resolved against the running build's own tag. Read and dropped before any
/// await — a settings guard must not be held across one.
fn update_channel(app: &AppHandle, state: &State<'_, AppState>) -> Result<update::Channel> {
    let pref = state
        .settings
        .lock()
        .map(|settings| settings.update_channel.clone())
        .map_err(|_| poisoned())?;
    Ok(update::channel_for(app, &pref))
}

/// Whether the in-app updater can service this install at all, and whether a
/// bundle is already staged for the next quit. One command because the UI needs
/// both before it can draw anything.
#[tauri::command]
pub fn update_state(app: AppHandle) -> UpdateState {
    UpdateState {
        version: app.package_info().version.to_string(),
        support: update::install_support(),
        staged: update::staged(&app),
    }
}

/// What the update surfaces need before a check: which version is running,
/// whether this install can self-update, and whether one is already staged.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateState {
    pub version: String,
    pub support: update::InstallSupport,
    pub staged: bool,
}

/// Is a newer build published on the channel this install polls?
#[tauri::command]
pub async fn check_for_update(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<update::UpdateMeta>> {
    let channel = update_channel(&app, &state)?;
    update::check_for_update(app, channel).await
}

/// Download and verify the update, then hold it for the next quit. Nothing is
/// replaced on disk until then.
#[tauri::command]
pub async fn stage_update(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<update::UpdateMeta> {
    let channel = update_channel(&app, &state)?;
    update::stage_update(app, channel).await
}

/// Install the staged bundle now and relaunch into it.
#[tauri::command]
pub fn restart_and_install(app: AppHandle) -> Result<()> {
    update::restart_and_install(&app)
}

// ─── Tabs ───────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn new_tab(app: AppHandle, url: Option<String>) -> Result<u32> {
    site::new_tab(&app, url)
}

#[tauri::command]
pub fn close_tab(app: AppHandle, id: u32) -> Result<()> {
    site::close_tab(&app, id)
}

#[tauri::command]
pub fn activate_tab(app: AppHandle, id: u32) -> Result<()> {
    site::activate_tab(&app, id)
}

// ─── The store and the tools ────────────────────────────────────────────────

#[tauri::command]
pub fn get_store_counts(db: State<'_, Db>) -> Result<Counts> {
    db.counts()
}

#[tauri::command]
pub fn list_people(db: State<'_, Db>, filter: UserFilter) -> Result<Vec<User>> {
    db.users(&filter)
}

#[tauri::command]
pub fn list_posts(db: State<'_, Db>, filter: PostFilter) -> Result<Vec<Post>> {
    db.posts(&filter)
}

/// Returns the path written, or `None` when the dialog was dismissed. Async,
/// so the blocking dialog runs off the main thread it would otherwise stall.
#[tauri::command]
pub async fn export_people(
    app: AppHandle,
    db: State<'_, Db>,
    filter: UserFilter,
    format: String,
) -> Result<Option<String>> {
    let format = export::Format::parse(&format)?;
    let users = db.users(&filter)?;
    let contents = export::render_users(&users, format)?;
    export::save(
        &app,
        &format!("twister-people.{}", format.extension()),
        &contents,
    )
}

#[tauri::command]
pub async fn export_posts(
    app: AppHandle,
    db: State<'_, Db>,
    filter: PostFilter,
    format: String,
) -> Result<Option<String>> {
    let format = export::Format::parse(&format)?;
    let name = if filter.source.as_deref() == Some("Bookmarks") {
        "twister-bookmarks"
    } else {
        "twister-posts"
    };
    let posts = db.posts(&filter)?;
    let contents = export::render_posts(&posts, format)?;
    export::save(&app, &format!("{name}.{}", format.extension()), &contents)
}

#[tauri::command]
pub fn clear_captured(db: State<'_, Db>) -> Result<()> {
    db.clear_captured()
}

#[tauri::command]
pub fn start_op(app: AppHandle, kind: String, params: Value, dry_run: bool) -> Result<Job> {
    ops::start(&app, &kind, params, dry_run, "app", None)
}

#[tauri::command]
pub fn cancel_op(app: AppHandle) -> Result<()> {
    ops::cancel(&app)
}

#[tauri::command]
pub fn get_ops(app: AppHandle) -> Result<ops::OpsState> {
    ops::state(&app)
}

#[tauri::command]
pub fn prepare_post(markdown: String) -> compose::Prepared {
    compose::prepare(&markdown)
}

#[tauri::command]
pub fn post_now(app: AppHandle, markdown: String) -> Result<Job> {
    let prepared = compose::prepare(&markdown);
    if prepared.parts.is_empty() {
        return Err(AppError::InvalidInput("Nothing to post.".into()));
    }
    let parts: Vec<String> = prepared.parts.into_iter().map(|p| p.text).collect();
    ops::start(
        &app,
        "compose",
        serde_json::json!({ "parts": parts }),
        false,
        "app",
        None,
    )
}

#[tauri::command]
pub fn schedule_post(
    app: AppHandle,
    db: State<'_, Db>,
    markdown: String,
    scheduled_at: String,
) -> Result<ScheduledPost> {
    let when = chrono::DateTime::parse_from_rfc3339(&scheduled_at)
        .map_err(|e| AppError::InvalidInput(format!("That is not a time: {e}")))?;
    if when < chrono::Utc::now() {
        return Err(AppError::InvalidInput("That time has passed.".into()));
    }
    let prepared = compose::prepare(&markdown);
    if prepared.parts.is_empty() {
        return Err(AppError::InvalidInput("Nothing to post.".into()));
    }
    let parts: Vec<String> = prepared.parts.into_iter().map(|p| p.text).collect();
    let post = db.schedule_post(
        &parts,
        &crate::scheduled_format(when.with_timezone(&chrono::Utc)),
    )?;
    scheduler::changed(&app);
    Ok(post)
}

#[tauri::command]
pub fn list_scheduled_posts(db: State<'_, Db>) -> Result<Vec<ScheduledPost>> {
    db.scheduled_posts()
}

#[tauri::command]
pub fn delete_scheduled_post(app: AppHandle, db: State<'_, Db>, id: i64) -> Result<()> {
    db.delete_scheduled_post(id)?;
    scheduler::changed(&app);
    Ok(())
}

#[tauri::command]
pub fn open_downloads_dir() -> Result<()> {
    tauri_plugin_opener::open_path(download::dir()?, None::<&str>)
        .map_err(|err| internal("Opening the downloads folder", err))
}

/// Called by the tooltip page once it has rendered and measured its content.
#[tauri::command]
pub fn tooltip_ready(app: AppHandle, width: f64, height: f64) -> Result<()> {
    tooltip::ready(&app, width, height)
}

// ─── Bridge (callable from x.com) ───────────────────────────────────────────

#[tauri::command]
pub fn site_settings(state: State<'_, AppState>) -> Result<SitePrefs> {
    state
        .settings
        .lock()
        .map(|s| s.site_prefs())
        .map_err(|_| poisoned())
}

#[tauri::command]
pub fn site_navigated(app: AppHandle, webview: Webview, url: String) {
    site::bridge_navigated(&app, webview.label(), &url);
}

#[tauri::command]
pub fn site_profile(app: AppHandle, handle: String) -> Result<()> {
    site::bridge_profile(&app, &handle)
}

/// What X loaded into a page, batched. Answers with how many rows were kept.
#[tauri::command]
pub fn site_capture(
    state: State<'_, AppState>,
    db: State<'_, Db>,
    batch: capture::Batch,
) -> Result<usize> {
    let enabled = state
        .settings
        .lock()
        .map(|s| s.niceties.capture)
        .map_err(|_| poisoned())?;
    if !enabled {
        return Ok(0);
    }
    let (users, posts, dropped) = capture::sanitize(batch)?;
    if dropped > 0 {
        log::debug!("capture dropped {dropped} malformed rows");
    }
    let kept = db.record_users(&users)? + db.record_posts(&posts)?;
    Ok(kept)
}

#[tauri::command]
pub fn site_op_progress(
    app: AppHandle,
    webview: Webview,
    id: i64,
    progress: ops::Progress,
    removed: Option<Vec<String>>,
) -> Result<()> {
    ops::report_from(
        &app,
        webview.label(),
        id,
        progress,
        &removed.unwrap_or_default(),
    )
}

#[tauri::command]
pub fn site_download(app: AppHandle, request: download::Request) -> Result<()> {
    download::start(&app, request)
}

#[tauri::command]
pub fn site_layout(
    app: AppHandle,
    webview: Webview,
    min_width: f64,
    detail: Option<String>,
) -> Result<()> {
    if let Some(detail) = detail {
        log::debug!(
            "{} layout: {}",
            webview.label(),
            detail.chars().take(2000).collect::<String>()
        );
    }
    site::bridge_layout(&app, webview.label(), min_width)
}
