//! Twister — a desktop client for X with niceties injected.
//!
//! One window, two child webviews: the shell (this app's React frame) and the
//! site (x.com, with a bridge script). The window is created here rather than
//! in `tauri.conf.json` because a configured window is a single-webview
//! window, and the whole design rests on there being two.

mod commands;
mod error;
mod menu;
mod settings;
mod site;
mod windowing;

use std::sync::Mutex;
use std::time::Duration;

use tauri::utils::config::{LogicalPosition as ConfigPosition, WindowConfig};
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
const MIN_SIZE: (f64, f64) = (880.0, 580.0);

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
        .menu(menu::build)
        .on_menu_event(menu::handle)
        .setup(setup)
        .on_window_event(|window, event| match event {
            // Closing hides: the session stays warm, the badge keeps counting,
            // and on macOS closing a window has never meant quitting anyway.
            WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                remember_bounds(window.app_handle());
                persist_bounds(window.app_handle());
                let _ = window.hide();
            }
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                if let Err(err) = site::layout(window.app_handle()) {
                    log::debug!("layout skipped: {err}");
                }
                remember_bounds(window.app_handle());
            }
            WindowEvent::Moved(_) => remember_bounds(window.app_handle()),
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
            commands::site_settings,
            commands::site_navigated,
            commands::site_profile,
        ])
        .build(tauri::generate_context!())
        .expect("Twister failed to start")
        .run(|app, event| match event {
            // Clicking the Dock icon does not start a second process, so the
            // single-instance handler never fires for it; this is the one
            // event that brings a hidden window back.
            RunEvent::Reopen { .. } => show_main_window(app),
            RunEvent::ExitRequested { .. } => persist_bounds(app),
            _ => {}
        });
}

fn setup(app: &mut tauri::App) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let store = Store::open()?;
    let settings = store.load_settings();
    let saved = store.load_window();
    log::info!("settings in {}", settings::data_dir()?.display());

    app.manage(AppState {
        site: Site::new(settings.niceties),
        settings: Mutex::new(settings.clone()),
        bounds: Mutex::new(saved),
        store,
    });

    let window = build_window(app, saved)?;
    windowing::apply_material(&window, &settings.window_material);

    let scale = window.scale_factor()?;
    let size: LogicalSize<f64> = window.inner_size()?.to_logical(scale);
    let shell = WebviewBuilder::new(site::SHELL_LABEL, WebviewUrl::App("index.html".into()))
        .transparent(true)
        .devtools(cfg!(debug_assertions))
        .auto_resize();
    window.add_child(shell, LogicalPosition::new(0.0, 0.0), size)?;

    // Added second, so it sits above the shell. The island is its territory.
    site::build(app.handle(), &window, settings.niceties)?;

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

/// A plain window (no webview of its own), from a config so the macOS traffic
/// lights can be placed — the builder API has no setter for that.
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
        traffic_light_position: Some(ConfigPosition { x: 18.0, y: 20.0 }),
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

fn remember_bounds(app: &AppHandle) {
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
        if maximized {
            if let Some(current) = bounds.as_mut() {
                current.maximized = true;
            }
        } else {
            *bounds = Some(WindowBounds {
                x: position.x,
                y: position.y,
                width: size.width,
                height: size.height,
                maximized: false,
            });
        }
    }
}

fn persist_bounds(app: &AppHandle) {
    let state = app.state::<AppState>();
    let bounds = state.bounds.lock().ok().and_then(|b| *b);
    if let Some(bounds) = bounds
        && let Err(err) = state.store.save_window(&bounds)
    {
        log::warn!("could not save the window placement: {err}");
    }
}
