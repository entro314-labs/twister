//! Every Tauri command, in one place. The shell's and the bridge's commands are
//! kept apart by the capability files, not by this module: `site_*` are the
//! three x.com may call.

use std::sync::Mutex;

use tauri::{AppHandle, Manager, State};

use crate::error::{AppError, Result};
use crate::settings::{Niceties, Settings, Store, WindowBounds};
use crate::site::{self, Action, Destination, Insets, Site, SiteState};

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
    site::apply_niceties(&app, settings.niceties)?;
    Ok(settings)
}

#[tauri::command]
pub fn get_site_state(state: State<'_, AppState>) -> Result<SiteState> {
    state
        .site
        .state
        .lock()
        .map(|s| s.clone())
        .map_err(|_| poisoned())
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

// ─── Bridge (callable from x.com) ───────────────────────────────────────────

#[tauri::command]
pub fn site_settings(state: State<'_, AppState>) -> Result<Niceties> {
    state
        .settings
        .lock()
        .map(|s| s.niceties)
        .map_err(|_| poisoned())
}

#[tauri::command]
pub fn site_navigated(app: AppHandle, url: String) {
    site::bridge_navigated(&app, &url);
}

#[tauri::command]
pub fn site_profile(app: AppHandle, handle: String) -> Result<()> {
    site::bridge_profile(&app, &handle)
}
