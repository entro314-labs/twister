//! The tooltip layer.
//!
//! The site webview sits above the shell, so a tooltip the shell draws is
//! hidden the moment it reaches the island — which every sidebar and titlebar
//! tooltip does. Native `title` tooltips clear the island but cannot be
//! styled. So tooltips are their own tiny window: frameless, transparent, a
//! child of the main window so it rides along and stays above it, ignoring
//! the cursor so it never steals a hover. The shell asks for one with the
//! anchor's rectangle; the tooltip page renders the content, measures itself,
//! and reports back; only then is the window sized, placed and shown.

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{
    AppHandle, Emitter, EventTarget, LogicalSize, Manager, PhysicalPosition, WebviewUrl,
    WebviewWindow, WebviewWindowBuilder,
};

use crate::error::{AppError, Result};

pub const LABEL: &str = "tooltip";
/// Content for the tooltip page. Payload: [`Content`].
pub const EVENT_CONTENT: &str = "twister://tooltip";

/// Where the tooltip goes relative to its anchor.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Side {
    Right,
    Bottom,
}

/// The anchor's rectangle in the shell's logical coordinates, which — with
/// an overlay titlebar and a full-size content view — are the window's.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Anchor {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub side: Side,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Content {
    pub label: String,
    pub shortcut: Option<String>,
    /// `light` | `dark`, as the shell resolved it — the page has no other way
    /// to match.
    pub theme: String,
}

#[derive(Default)]
pub struct Tooltip {
    /// The anchor of the request in flight, consumed by `ready`. A hide in
    /// between clears it, so a late measurement cannot resurrect a tooltip.
    pending: Mutex<Option<Anchor>>,
}

const GAP: f64 = 6.0;

pub fn create(app: &AppHandle, main: &tauri::Window) -> tauri::Result<WebviewWindow> {
    let builder = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("tooltip.html".into()))
        .title("Tooltip")
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .skip_taskbar(true)
        .visible(false)
        .focused(false)
        .focusable(false)
        .accept_first_mouse(false)
        .inner_size(10.0, 10.0);
    let builder = parent(builder, main);
    let window = builder.build()?;
    window.set_ignore_cursor_events(true)?;
    Ok(window)
}

/// A child of the main window where the platform has the notion, so it moves
/// with it and never falls behind it; always-on-top elsewhere.
#[cfg(target_os = "macos")]
fn parent<'a>(
    builder: WebviewWindowBuilder<'a, tauri::Wry, AppHandle>,
    main: &tauri::Window,
) -> WebviewWindowBuilder<'a, tauri::Wry, AppHandle> {
    match main.ns_window() {
        Ok(ns_window) => builder.parent_raw(ns_window),
        Err(_) => builder.always_on_top(true),
    }
}

#[cfg(target_os = "windows")]
fn parent<'a>(
    builder: WebviewWindowBuilder<'a, tauri::Wry, AppHandle>,
    main: &tauri::Window,
) -> WebviewWindowBuilder<'a, tauri::Wry, AppHandle> {
    match main.hwnd() {
        Ok(hwnd) => builder.parent_raw(hwnd),
        Err(_) => builder.always_on_top(true),
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn parent<'a>(
    builder: WebviewWindowBuilder<'a, tauri::Wry, AppHandle>,
    _main: &tauri::Window,
) -> WebviewWindowBuilder<'a, tauri::Wry, AppHandle> {
    builder.always_on_top(true)
}

fn window(app: &AppHandle) -> Result<WebviewWindow> {
    app.get_webview_window(LABEL)
        .ok_or_else(|| AppError::NotFound("The tooltip window is gone.".into()))
}

pub fn show(app: &AppHandle, anchor: Anchor, content: Content) -> Result<()> {
    if content.label.trim().is_empty() {
        return Err(AppError::InvalidInput("A tooltip needs a label.".into()));
    }
    if [anchor.x, anchor.y, anchor.width, anchor.height]
        .iter()
        .any(|v| !v.is_finite())
    {
        return Err(AppError::InvalidInput("The anchor must be finite.".into()));
    }
    {
        let state = app.state::<Tooltip>();
        let mut pending = state
            .pending
            .lock()
            .map_err(|_| AppError::Internal("Tooltip lock poisoned.".into()))?;
        *pending = Some(anchor);
    }
    app.emit_to(EventTarget::webview_window(LABEL), EVENT_CONTENT, content)?;
    Ok(())
}

/// The page has rendered and measured: size the window to it, place it by
/// the anchor, and show it.
pub fn ready(app: &AppHandle, width: f64, height: f64) -> Result<()> {
    if !(width.is_finite() && height.is_finite()) || width <= 0.0 || height <= 0.0 {
        return Err(AppError::InvalidInput(
            "The tooltip size must be positive.".into(),
        ));
    }
    let anchor = {
        let state = app.state::<Tooltip>();
        let mut pending = state
            .pending
            .lock()
            .map_err(|_| AppError::Internal("Tooltip lock poisoned.".into()))?;
        pending.take()
    };
    // Hidden since the request: nothing to show.
    let Some(anchor) = anchor else {
        return Ok(());
    };
    let main = app
        .get_window(crate::MAIN_WINDOW)
        .ok_or_else(|| AppError::NotFound("The main window is gone.".into()))?;
    let scale = main.scale_factor()?;
    let origin = main.outer_position()?;
    let (x, y) = place(anchor, width, height);
    let tooltip = window(app)?;
    tooltip.set_size(LogicalSize::new(width, height))?;
    tooltip.set_position(PhysicalPosition::new(
        origin.x + to_physical(x, scale),
        origin.y + to_physical(y, scale),
    ))?;
    tooltip.show()?;
    Ok(())
}

pub fn hide(app: &AppHandle) -> Result<()> {
    if let Ok(mut pending) = app.state::<Tooltip>().pending.lock() {
        *pending = None;
    }
    window(app)?.hide()?;
    Ok(())
}

/// The tooltip's top-left, in the window's logical coordinates.
pub fn place(anchor: Anchor, width: f64, height: f64) -> (f64, f64) {
    match anchor.side {
        Side::Right => (
            anchor.x + anchor.width + GAP,
            anchor.y + (anchor.height - height) / 2.0,
        ),
        Side::Bottom => (
            anchor.x + (anchor.width - width) / 2.0,
            anchor.y + anchor.height + GAP,
        ),
    }
}

#[allow(clippy::cast_possible_truncation)]
fn to_physical(logical: f64, scale: f64) -> i32 {
    (logical * scale).round() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn right_tooltips_centre_on_the_anchor_and_clear_its_edge() {
        let anchor = Anchor {
            x: 10.0,
            y: 100.0,
            width: 36.0,
            height: 36.0,
            side: Side::Right,
        };
        let (x, y) = place(anchor, 80.0, 24.0);
        assert!((x - 52.0).abs() < f64::EPSILON);
        assert!((y - 106.0).abs() < f64::EPSILON);
    }

    #[test]
    fn bottom_tooltips_centre_under_the_anchor() {
        let anchor = Anchor {
            x: 100.0,
            y: 8.0,
            width: 28.0,
            height: 28.0,
            side: Side::Bottom,
        };
        let (x, y) = place(anchor, 60.0, 24.0);
        assert!((x - 84.0).abs() < f64::EPSILON);
        assert!((y - 42.0).abs() < f64::EPSILON);
    }

    #[test]
    fn physical_rounds_at_fractional_scales() {
        assert_eq!(to_physical(10.0, 2.0), 20);
        assert_eq!(to_physical(10.3, 1.5), 15);
    }
}
