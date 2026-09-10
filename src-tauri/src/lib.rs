//! Twister — a desktop client for X with niceties injected.
//!
//! One window, child webviews: the shell (this app's React frame) and one
//! site webview per tab (x.com, with the bridge, capture and operations
//! scripts). The window is created here rather than in `tauri.conf.json`
//! because a configured window is a single-webview window, and the whole
//! design rests on there being several.

mod capture;
mod commands;
mod compose;
pub mod db;
mod download;
mod error;
mod export;
pub mod mcp;
mod menu;
mod ops;
mod scheduler;
pub mod settings;
mod site;
mod tooltip;
mod update;
mod userland;
mod windowing;

use std::sync::Mutex;
use std::time::Duration;

use tauri::utils::config::WindowConfig;
use tauri::webview::WebviewBuilder;
use tauri::window::WindowBuilder;
use tauri::{
    AppHandle, LogicalPosition, LogicalSize, Manager, PhysicalPosition, PhysicalSize, RunEvent,
    TitleBarStyle, WebviewUrl, WindowEvent,
};

use commands::AppState;
use settings::{Store, WindowBounds};
use site::Site;

pub const MAIN_WINDOW: &str = "main";

const DEFAULT_SIZE: (f64, f64) = (1240.0, 820.0);
pub const MIN_SIZE: (f64, f64) = (880.0, 580.0);

/// How scheduled times are written: RFC 3339, UTC, to the second — a form
/// that sorts as text, which is how the store compares them.
pub fn scheduled_format(when: chrono::DateTime<chrono::Utc>) -> String {
    when.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(
        if cfg!(debug_assertions) {
            "twister=debug,warn"
        } else {
            "twister=info,warn"
        },
    ))
    .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main_window(app);
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .menu(menu::build)
        .on_menu_event(menu::handle)
        .setup(setup)
        .on_window_event(|window, event| match event {
            // Closing hides: the session stays warm, the badge keeps counting,
            // and on macOS closing a window has never meant quitting anyway.
            WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                remember_bounds(window.app_handle());
                persist(window.app_handle());
                let _ = window.hide();
            }
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                if let Err(err) = site::layout(window.app_handle()) {
                    log::debug!("layout skipped: {err}");
                }
                remember_bounds(window.app_handle());
            }
            WindowEvent::Moved(_) => remember_bounds(window.app_handle()),
            // A tooltip left showing over another app is the one way this
            // window could look broken while it is not even in front.
            WindowEvent::Focused(false) => {
                let _ = tooltip::hide(window.app_handle());
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::update_settings,
            commands::get_site_state,
            commands::navigate_site,
            commands::site_action,
            commands::set_site_insets,
            commands::set_site_visible,
            commands::sign_out,
            commands::shell_ready,
            commands::set_window_material,
            commands::list_user_assets,
            commands::open_user_assets_dir,
            commands::reload_site,
            commands::show_tooltip,
            commands::hide_tooltip,
            commands::new_tab,
            commands::close_tab,
            commands::activate_tab,
            commands::get_store_counts,
            commands::list_people,
            commands::list_posts,
            commands::export_people,
            commands::export_posts,
            commands::clear_captured,
            commands::start_op,
            commands::cancel_op,
            commands::get_ops,
            commands::prepare_post,
            commands::post_now,
            commands::schedule_post,
            commands::list_scheduled_posts,
            commands::delete_scheduled_post,
            commands::open_downloads_dir,
            commands::update_state,
            commands::check_for_update,
            commands::stage_update,
            commands::restart_and_install,
            commands::tooltip_ready,
            commands::site_settings,
            commands::site_navigated,
            commands::site_profile,
            commands::site_capture,
            commands::site_op_progress,
            commands::site_download,
            commands::site_layout,
        ])
        .build(tauri::generate_context!())
        .expect("Twister failed to start")
        .run(|app, event| match event {
            // Clicking the Dock icon does not start a second process, so the
            // single-instance handler never fires for it; this is the one
            // event that brings a hidden window back. There is no Dock off
            // macOS and no `Reopen` variant either, so the arm itself has to
            // go — a `#[cfg]` at the use site is the only form that compiles
            // on a platform where the variant does not exist.
            #[cfg(target_os = "macos")]
            RunEvent::Reopen { .. } => show_main_window(app),
            // The quit path, and the only one: closing the window hides it, so
            // this fires on ⌘Q and Quit — precisely when no code is running out
            // of the bundle a staged update replaces.
            RunEvent::ExitRequested { .. } => {
                persist(app);
                update::install_pending_on_exit(app);
            }
            _ => {}
        });
}

fn setup(app: &mut tauri::App) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let store = Store::open()?;
    let settings = store.load_settings();
    let saved = store.load_window();
    let tabs = store.load_tabs();
    let dir = settings::data_dir()?;
    log::info!("settings in {}", dir.display());

    let db = db::Db::open_at(&dir.join(db::DB_FILE))?;
    let left_running = db.settle_stale_jobs()?;
    if left_running > 0 {
        log::info!("{left_running} job(s) were left running by the last quit");
    }
    app.manage(db);
    app.manage(ops::Ops::default());
    app.manage(AppState {
        site: Site::new(settings.site_prefs()),
        settings: Mutex::new(settings.clone()),
        bounds: Mutex::new(saved),
        store,
    });
    app.manage(tooltip::Tooltip::default());
    app.manage(update::PendingUpdate::default());

    let window = build_window(app, saved)?;
    // Before the material and before any child webview: the style-mask and
    // toolbar changes reshape the title bar, and the shell measures against it.
    #[cfg(target_os = "macos")]
    windowing::apply_macos_chrome(&window);
    windowing::apply_material(&window, &settings.window_material);

    let scale = window.scale_factor()?;
    let size: LogicalSize<f64> = window.inner_size()?.to_logical(scale);
    let shell = WebviewBuilder::new(site::SHELL_LABEL, WebviewUrl::App("index.html".into()))
        .transparent(true)
        .devtools(cfg!(debug_assertions))
        .auto_resize();
    window.add_child(shell, LogicalPosition::new(0.0, 0.0), size)?;

    // Added after the shell, so the tabs sit above it. The island is theirs.
    site::restore_tabs(app.handle(), &window, &tabs)?;
    tooltip::create(app.handle(), &window)?;
    app.manage(scheduler::Scheduler::start(app.handle().clone()));

    // The shell shows the window once it has painted. If it never does — a
    // dev server that is not running, a broken build — a window is still
    // better than a process with no way in.
    let handle = app.handle().clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(3));
        show_main_window(&handle);
    });
    Ok(())
}

/// A plain window (no webview of its own). Built from a `WindowConfig` because
/// that is the only shape `hidden_title` is settable through; the macOS title
/// bar itself is finished in `windowing::apply_macos_chrome`.
fn build_window(app: &tauri::App, saved: Option<WindowBounds>) -> tauri::Result<tauri::Window> {
    let config = WindowConfig {
        label: MAIN_WINDOW.into(),
        title: "Twister".into(),
        width: DEFAULT_SIZE.0,
        height: DEFAULT_SIZE.1,
        min_width: Some(MIN_SIZE.0),
        min_height: Some(MIN_SIZE.1),
        visible: false,
        transparent: true,
        title_bar_style: TitleBarStyle::Overlay,
        hidden_title: true,
        ..WindowConfig::default()
    };
    let window = WindowBuilder::from_config(app, &config)?.build()?;

    if let Some(bounds) = saved {
        // Restored in physical pixels, exactly as recorded. A window that was
        // dragged off-screen comes back wherever the OS clamps it to.
        let _ = window.set_position(PhysicalPosition::new(bounds.x, bounds.y));
        let _ = window.set_size(PhysicalSize::new(bounds.width, bounds.height));
        if bounds.maximized {
            let _ = window.maximize();
        }
    } else {
        let _ = window.center();
    }
    Ok(window)
}

pub fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_window(MAIN_WINDOW) {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

pub fn remember_bounds(app: &AppHandle) {
    let Some(window) = app.get_window(MAIN_WINDOW) else {
        return;
    };
    let maximized = window.is_maximized().unwrap_or(false);
    let (Ok(position), Ok(size)) = (window.outer_position(), window.inner_size()) else {
        return;
    };
    // A minimized or maximized window reports geometry that must not overwrite
    // the one to restore to.
    if window.is_minimized().unwrap_or(false) {
        return;
    }
    let state = app.state::<AppState>();
    if let Ok(mut bounds) = state.bounds.lock() {
        // A maximized window reports the screen's geometry, which is not what
        // to restore to. Keep the last free-floating rectangle when there is
        // one and only flip the flag; a window maximized before it was ever
        // moved has nothing better to remember than what it reports now.
        *bounds = match (*bounds, maximized) {
            (Some(previous), true) => Some(WindowBounds {
                maximized: true,
                ..previous
            }),
            _ => Some(WindowBounds {
                x: position.x,
                y: position.y,
                width: size.width,
                height: size.height,
                maximized,
            }),
        };
    }
}

/// Writes the window placement and the open tabs.
fn persist(app: &AppHandle) {
    let state = app.state::<AppState>();
    let bounds = state.bounds.lock().ok().and_then(|b| *b);
    if let Some(bounds) = bounds
        && let Err(err) = state.store.save_window(&bounds)
    {
        log::warn!("could not save the window placement: {err}");
    }
    if let Err(err) = state.store.save_tabs(&site::saved_tabs(app)) {
        log::warn!("could not save the open tabs: {err}");
    }
}
