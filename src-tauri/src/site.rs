//! The site: x.com in a child webview, and everything the app knows about it.
//!
//! The shell webview owns the frame — sidebar, titlebar, status bar — and this
//! webview owns the island inside it. It sits ON TOP of the shell, so anything
//! the shell needs to show over the island hides the site first
//! (see `set_visible`). Position and size are derived from insets the shell
//! reports, re-applied by Rust on every window resize so the island never
//! lags the frame.
//!
//! Everything that touches X's DOM is in `site/bridge.js` and
//! `site/niceties.css`. This file only knows URLs and titles.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use serde::{Deserialize, Serialize};
use tauri::webview::{NewWindowResponse, PageLoadEvent, WebviewBuilder};
use tauri::{
    AppHandle, Emitter, EventTarget, LogicalPosition, LogicalSize, Manager, Webview, WebviewUrl,
    Window,
};
use url::Url;

use crate::error::{AppError, Result};
use crate::settings::Niceties;
use crate::userland;

pub const SHELL_LABEL: &str = "shell";
pub const HOME: &str = "https://x.com/home";

/// The whole site state, pushed to the shell whenever any of it changes.
pub const EVENT_STATE: &str = "twister://site-state";
/// A one-line notice for the status bar. Payload: [`Notice`].
pub const EVENT_NOTICE: &str = "twister://notice";

const BRIDGE_JS: &str = include_str!("../site/bridge.js");
const NICETIES_CSS: &str = include_str!("../site/niceties.css");

/// Hosts a navigation may go to inside the webview. Everything else opens in
/// the system browser. `on_navigation` fires for iframes as well as the main
/// frame, so the sign-in providers X embeds are here too — denying them would
/// blank the buttons on the login page.
const ALLOWED_HOSTS: &[&str] = &[
    "x.com",
    "twitter.com",
    "twimg.com",
    "t.co",
    "accounts.google.com",
    "appleid.apple.com",
];

// ─── Types ──────────────────────────────────────────────────────────────────

/// Mirrored by `SiteState` in the renderer.
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SiteState {
    pub url: String,
    pub section: Section,
    /// The page title with X's own decoration stripped: "(3) Home / X" → "Home".
    pub title: String,
    /// Parsed from the title; X prefixes it with the unread count.
    pub unread: u32,
    pub loading: bool,
    /// The signed-in handle, once the bridge has seen X's profile link.
    pub handle: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Section {
    Home,
    Explore,
    Notifications,
    Messages,
    Bookmarks,
    Profile,
    Compose,
    #[default]
    Other,
}

/// Where the sidebar, the menu and the shortcuts can send the site.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Destination {
    Home,
    Explore,
    Notifications,
    Messages,
    Bookmarks,
    Profile,
    Compose,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Action {
    Back,
    Forward,
    Reload,
}

/// The frame around the island, in logical pixels, as the shell measures it.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Insets {
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
}

impl Default for Insets {
    /// Matches the shell's default layout so the first frame is right before
    /// the shell has measured anything.
    fn default() -> Self {
        Self {
            left: 236.0,
            top: 44.0,
            right: 0.0,
            bottom: 24.0,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Notice {
    pub message: String,
}

pub struct Site {
    pub state: Mutex<SiteState>,
    pub insets: Mutex<Insets>,
    /// The last niceties applied, so a fresh page load bakes them in.
    pub niceties: Mutex<Niceties>,
    /// Whether the shell wants the site showing. Re-applied after a rebuild.
    pub visible: AtomicBool,
    /// The webview's current label. A rebuild closes the old webview and adds
    /// a new one, and the manager only forgets a label once the close has
    /// gone through — so every build gets a fresh one (`site-1`, `site-2`…),
    /// and the capability file grants `site-*`.
    pub label: Mutex<String>,
    generation: AtomicU32,
}

impl Site {
    pub fn new(niceties: Niceties) -> Self {
        Self {
            state: Mutex::new(SiteState {
                url: HOME.into(),
                section: Section::Home,
                loading: true,
                ..SiteState::default()
            }),
            insets: Mutex::new(Insets::default()),
            niceties: Mutex::new(niceties),
            visible: AtomicBool::new(true),
            label: Mutex::new(String::new()),
            generation: AtomicU32::new(0),
        }
    }

    fn next_label(&self) -> String {
        let n = self.generation.fetch_add(1, Ordering::Relaxed) + 1;
        let label = format!("site-{n}");
        if let Ok(mut current) = self.label.lock() {
            current.clone_from(&label);
        }
        label
    }
}

// ─── Pure functions ─────────────────────────────────────────────────────────

/// Whether a navigation may happen inside the webview. Redirect hops each pass
/// through here, so a `t.co` link is allowed and the external page it resolves
/// to is not — which is exactly when it should leave for the browser.
pub fn allows(url: &Url) -> bool {
    match url.scheme() {
        "about" | "blob" | "data" => return true,
        "http" | "https" => {}
        _ => return false,
    }
    let Some(host) = url.host_str() else {
        return false;
    };
    ALLOWED_HOSTS
        .iter()
        .any(|allowed| host == *allowed || host.ends_with(&format!(".{allowed}")))
}

/// The unread count X puts at the front of every real page title, or `None`
/// when the title is one of the transient ones ("X", "") that carry no
/// information and must not reset the badge.
pub fn unread_from_title(title: &str) -> Option<u32> {
    let title = title.trim();
    if !title.ends_with("/ X") {
        return None;
    }
    let Some(rest) = title.strip_prefix('(') else {
        return Some(0);
    };
    let (count, _) = rest.split_once(')')?;
    count.trim().parse().ok()
}

/// "(3) Home / X" → "Home".
pub fn clean_title(title: &str) -> String {
    let mut title = title.trim();
    if title.starts_with('(')
        && let Some((_, rest)) = title.split_once(')')
    {
        title = rest.trim_start();
    }
    title
        .strip_suffix("/ X")
        .map_or(title, str::trim_end)
        .to_string()
}

pub fn section_for(url: &str, handle: Option<&str>) -> Section {
    let Ok(parsed) = Url::parse(url) else {
        return Section::Other;
    };
    let path = parsed.path().trim_end_matches('/');
    match path {
        "/home" | "" => Section::Home,
        "/explore" | "/search" => Section::Explore,
        "/notifications" => Section::Notifications,
        "/i/bookmarks" => Section::Bookmarks,
        "/compose/post" => Section::Compose,
        _ if path.starts_with("/explore/")
            || path.starts_with("/search")
            || path.starts_with("/i/trends") =>
        {
            Section::Explore
        }
        _ if path.starts_with("/notifications/") => Section::Notifications,
        _ if path.starts_with("/messages") || path.starts_with("/i/chat") => Section::Messages,
        _ => match handle {
            Some(handle)
                if path
                    .strip_prefix('/')
                    .is_some_and(|rest| rest.split('/').next() == Some(handle)) =>
            {
                Section::Profile
            }
            _ => Section::Other,
        },
    }
}

pub fn destination_url(destination: Destination, handle: Option<&str>) -> Result<String> {
    Ok(match destination {
        Destination::Home => HOME.into(),
        Destination::Explore => "https://x.com/explore".into(),
        Destination::Notifications => "https://x.com/notifications".into(),
        // X moved its messages to /i/chat; /messages still redirects there.
        Destination::Messages => "https://x.com/i/chat".into(),
        Destination::Bookmarks => "https://x.com/i/bookmarks".into(),
        Destination::Compose => "https://x.com/compose/post".into(),
        Destination::Profile => {
            let handle = handle.ok_or_else(|| {
                AppError::InvalidInput(
                    "Twister has not seen your handle yet. Open X's home first.".into(),
                )
            })?;
            format!("https://x.com/{handle}")
        }
    })
}

/// An X handle is 1–15 word characters. Anything else from the bridge is
/// discarded — the page it runs on is not trusted.
pub fn valid_handle(handle: &str) -> bool {
    !handle.is_empty()
        && handle.len() <= 15
        && handle
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// Where the island goes for a window of this size. Never negative, so a
/// window squeezed below the frame's own size clamps rather than errors.
pub fn bounds_for(
    insets: Insets,
    window: LogicalSize<f64>,
) -> (LogicalPosition<f64>, LogicalSize<f64>) {
    let width = (window.width - insets.left - insets.right).max(1.0);
    let height = (window.height - insets.top - insets.bottom).max(1.0);
    (
        LogicalPosition::new(insets.left, insets.top),
        LogicalSize::new(width, height),
    )
}

fn init_script(niceties: Niceties, user_styles: &[(String, String)]) -> String {
    // Every substitution is JSON, which is valid JavaScript for a string, an
    // array and an object literal alike.
    BRIDGE_JS
        .replace(
            "__TWISTER_CSS__",
            &serde_json::to_string(NICETIES_CSS).unwrap_or_else(|_| "\"\"".into()),
        )
        .replace(
            "__TWISTER_USER_CSS__",
            &serde_json::to_string(user_styles).unwrap_or_else(|_| "[]".into()),
        )
        .replace(
            "__TWISTER_NICETIES__",
            &serde_json::to_string(&niceties).unwrap_or_else(|_| "{}".into()),
        )
}

// ─── Wiring ─────────────────────────────────────────────────────────────────

/// Creates the site webview as a child of the window. Bridge commands and
/// events reach the state through the app handle, which is why the
/// [`Site`] must already be managed when this runs.
pub fn build(app: &AppHandle, window: &Window, niceties: Niceties) -> tauri::Result<Webview> {
    let scale = window.scale_factor()?;
    let size: LogicalSize<f64> = window.inner_size()?.to_logical(scale);
    let insets = site(app)
        .site
        .insets
        .lock()
        .map_or_else(|_| Insets::default(), |i| *i);
    let (position, bounds) = bounds_for(insets, size);
    let label = site(app).site.next_label();

    // The user's own scripts and styles, read fresh on every build: a reload
    // from Settings is a rebuild, and this is where it picks the changes up.
    let assets = userland::load().unwrap_or_else(|err| {
        log::warn!("user scripts and styles unavailable: {err}");
        userland::Loaded::default()
    });

    let start = Url::parse(HOME).map_err(tauri::Error::InvalidUrl)?;

    let nav_handle = app.clone();
    let new_window_handle = app.clone();
    let title_handle = app.clone();
    let load_handle = app.clone();

    let mut builder = WebviewBuilder::new(&label, WebviewUrl::External(start))
        .initialization_script(init_script(niceties, &assets.styles));
    for (name, source) in &assets.scripts {
        builder = builder.initialization_script(userland::wrap_script(name, source));
    }
    let builder = builder
        .devtools(cfg!(debug_assertions))
        .on_navigation(move |url| {
            if allows(url) {
                return true;
            }
            open_external(&nav_handle, url);
            false
        })
        .on_new_window(move |url, _features| {
            // A popup is either an X page — shown in place — or a link out.
            // Either way no second window: a client is one frame.
            if allows(&url) {
                if let Err(err) = navigate(&new_window_handle, url.as_str()) {
                    log::warn!("could not open {url} in place: {err}");
                }
            } else {
                open_external(&new_window_handle, &url);
            }
            NewWindowResponse::Deny
        })
        .on_document_title_changed(move |_, title| {
            update(&title_handle, |state| {
                let cleaned = clean_title(&title);
                if !cleaned.is_empty() {
                    state.title = cleaned;
                }
                if let Some(unread) = unread_from_title(&title) {
                    state.unread = unread;
                }
            });
        })
        .on_page_load(move |_, payload| {
            let url = payload.url().to_string();
            let loading = matches!(payload.event(), PageLoadEvent::Started);
            log::debug!(
                "page load {}: {url}",
                if loading { "started" } else { "finished" }
            );
            update(&load_handle, |state| {
                state.section = section_for(&url, state.handle.as_deref());
                state.url = url;
                state.loading = loading;
            });
        });

    let webview = window.add_child(builder, position, bounds)?;
    if !site(app).site.visible.load(Ordering::Relaxed) {
        let _ = webview.hide();
    }
    Ok(webview)
}

/// Closes the site webview and creates it again, which is the only way to
/// change its initialization scripts — and therefore how a user script or
/// style added to the folder starts running.
pub fn rebuild(app: &AppHandle) -> Result<()> {
    let window = app
        .get_window(crate::MAIN_WINDOW)
        .ok_or_else(|| AppError::NotFound("The main window is gone.".into()))?;
    if let Ok(old) = webview(app) {
        old.close()?;
    }
    let niceties = site(app)
        .site
        .niceties
        .lock()
        .map_or_else(|_| Niceties::default(), |n| *n);
    update(app, |state| {
        state.loading = true;
        state.url = HOME.into();
        state.section = Section::Home;
    });
    build(app, &window, niceties)?;
    Ok(())
}

fn webview(app: &AppHandle) -> Result<Webview> {
    let label = site(app)
        .site
        .label
        .lock()
        .map_or_else(|_| String::new(), |l| l.clone());
    app.get_webview(&label)
        .ok_or_else(|| AppError::NotFound("The site webview is gone.".into()))
}

fn site(app: &AppHandle) -> tauri::State<'_, crate::commands::AppState> {
    app.state::<crate::commands::AppState>()
}

/// Mutates the state and pushes the result to the shell. Every change goes
/// through here so the shell and the Dock badge can never disagree.
pub fn update(app: &AppHandle, change: impl FnOnce(&mut SiteState)) {
    let snapshot = {
        let state = site(app);
        let Ok(mut current) = state.site.state.lock() else {
            return;
        };
        change(&mut current);
        current.clone()
    };
    let dock_badge = site(app)
        .site
        .niceties
        .lock()
        .map_or(true, |n| n.dock_badge);
    sync_badge(app, snapshot.unread, dock_badge);
    if let Err(err) = app.emit_to(EventTarget::webview(SHELL_LABEL), EVENT_STATE, snapshot) {
        log::warn!("could not push site state: {err}");
    }
}

pub fn sync_badge(app: &AppHandle, unread: u32, enabled: bool) {
    if let Some(window) = app.get_window(crate::MAIN_WINDOW) {
        let count = (enabled && unread > 0).then_some(i64::from(unread));
        // Unsupported on Windows; a badge that cannot be drawn is not an error.
        let _ = window.set_badge_count(count);
    }
}

pub fn notify(app: &AppHandle, message: impl Into<String>) {
    let _ = app.emit_to(
        EventTarget::webview(SHELL_LABEL),
        EVENT_NOTICE,
        Notice {
            message: message.into(),
        },
    );
}

fn open_external(app: &AppHandle, url: &Url) {
    match tauri_plugin_opener::open_url(url.as_str(), None::<&str>) {
        Ok(()) => notify(
            app,
            format!(
                "Opened {} in your browser",
                url.host_str().unwrap_or("the link")
            ),
        ),
        Err(err) => {
            log::warn!("could not open {url}: {err}");
            notify(app, "Could not open that link in your browser");
        }
    }
}

pub fn navigate(app: &AppHandle, url: &str) -> Result<()> {
    let parsed = Url::parse(url).map_err(|e| AppError::InvalidInput(format!("Bad URL: {e}")))?;
    if !allows(&parsed) {
        return Err(AppError::InvalidInput(format!(
            "{url} is not somewhere the site can go."
        )));
    }
    webview(app)?.navigate(parsed)?;
    Ok(())
}

/// Sends the site to a destination through X's own navigation when the bridge
/// is there to click it, and by a full load otherwise. The distinction is
/// what keeps a sidebar click instant — and what makes the compose modal open
/// at all: loaded cold, `/compose/post` never gets past X's splash screen.
pub fn go(app: &AppHandle, destination: Destination) -> Result<()> {
    let handle = site(app)
        .site
        .state
        .lock()
        .map(|state| state.handle.clone())
        .unwrap_or_default();
    let url = destination_url(destination, handle.as_deref())?;
    let parsed = Url::parse(&url).map_err(|e| AppError::Internal(e.to_string()))?;
    let path = serde_json::to_string(parsed.path())?;
    let href = serde_json::to_string(&url)?;
    webview(app)?.eval(format!(
        "(window.__twister && window.__twister.go({path})) || location.assign({href})"
    ))?;
    Ok(())
}

pub fn act(app: &AppHandle, action: Action) -> Result<()> {
    let webview = webview(app)?;
    match action {
        Action::Back => webview.eval("history.back()")?,
        Action::Forward => webview.eval("history.forward()")?,
        Action::Reload => webview.reload()?,
    }
    Ok(())
}

/// Re-derives the island's rectangle from the current window size and the
/// stored insets. Called on every resize and whenever the insets change.
pub fn layout(app: &AppHandle) -> Result<()> {
    let window = app
        .get_window(crate::MAIN_WINDOW)
        .ok_or_else(|| AppError::NotFound("The main window is gone.".into()))?;
    let insets = site(app)
        .site
        .insets
        .lock()
        .map(|insets| *insets)
        .unwrap_or_default();
    let scale = window.scale_factor()?;
    let size: LogicalSize<f64> = window.inner_size()?.to_logical(scale);
    let (position, bounds) = bounds_for(insets, size);
    let webview = webview(app)?;
    webview.set_position(position)?;
    webview.set_size(bounds)?;
    Ok(())
}

pub fn set_insets(app: &AppHandle, insets: Insets) -> Result<()> {
    if [insets.left, insets.top, insets.right, insets.bottom]
        .iter()
        .any(|v| !v.is_finite() || *v < 0.0)
    {
        return Err(AppError::InvalidInput(
            "Insets must be finite and non-negative.".into(),
        ));
    }
    let changed = {
        let state = site(app);
        let mut current = state
            .site
            .insets
            .lock()
            .map_err(|_| AppError::Internal("Insets lock poisoned.".into()))?;
        let changed = *current != insets;
        *current = insets;
        changed
    };
    if changed {
        layout(app)?;
    }
    Ok(())
}

pub fn set_visible(app: &AppHandle, visible: bool) -> Result<()> {
    site(app).site.visible.store(visible, Ordering::Relaxed);
    let webview = webview(app)?;
    if visible {
        webview.show()?;
    } else {
        webview.hide()?;
    }
    Ok(())
}

/// Pushes new niceties into the live page and remembers them for the next load.
pub fn apply_niceties(app: &AppHandle, niceties: Niceties) -> Result<()> {
    if let Ok(mut current) = site(app).site.niceties.lock() {
        *current = niceties;
    }
    let json = serde_json::to_string(&niceties)?;
    webview(app)?.eval(format!(
        "window.__twister && window.__twister.apply({json})"
    ))?;
    let unread = site(app).site.state.lock().map_or(0, |state| state.unread);
    sync_badge(app, unread, niceties.dock_badge);
    Ok(())
}

/// Forgets the X session: every cookie and every byte of site storage, then
/// back to the front door.
pub fn sign_out(app: &AppHandle) -> Result<()> {
    let webview = webview(app)?;
    webview.clear_all_browsing_data()?;
    update(app, |state| {
        state.handle = None;
        state.unread = 0;
        state.title.clear();
    });
    webview.navigate(Url::parse(HOME).map_err(|e| AppError::Internal(e.to_string()))?)?;
    Ok(())
}

// ─── Bridge ─────────────────────────────────────────────────────────────────
// Called from the page. Inputs are untrusted.

pub fn bridge_navigated(app: &AppHandle, url: &str) {
    let Ok(parsed) = Url::parse(url) else {
        return;
    };
    if !allows(&parsed) {
        return;
    }
    log::debug!("in-app navigation: {url}");
    update(app, |state| {
        state.section = section_for(url, state.handle.as_deref());
        state.url = url.to_string();
    });
}

pub fn bridge_profile(app: &AppHandle, handle: &str) -> Result<()> {
    if !valid_handle(handle) {
        return Err(AppError::InvalidInput("That is not an X handle.".into()));
    }
    update(app, |state| {
        state.handle = Some(handle.to_string());
        state.section = section_for(&state.url, Some(handle));
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(s: &str) -> Url {
        Url::parse(s).expect("test url")
    }

    #[test]
    fn allowlist_covers_x_and_its_sign_in_frames_only() {
        assert!(allows(&url("https://x.com/home")));
        assert!(allows(&url("https://twitter.com/home")));
        assert!(allows(&url("https://api.x.com/graphql")));
        assert!(allows(&url("https://pbs.twimg.com/media/a.jpg")));
        assert!(allows(&url("https://t.co/abc")));
        assert!(allows(&url("https://accounts.google.com/gsi/button")));
        assert!(allows(&url("about:blank")));
        assert!(allows(&url("about:srcdoc")));
        assert!(!allows(&url("https://example.com/")));
        assert!(!allows(&url("https://notx.com/")));
        assert!(!allows(&url("https://x.com.evil.example/")));
        assert!(!allows(&url("https://google.com/")));
        assert!(!allows(&url("mailto:a@b.c")));
        assert!(!allows(&url("ftp://x.com/")));
    }

    #[test]
    fn unread_is_read_from_real_titles_only() {
        assert_eq!(unread_from_title("(3) Home / X"), Some(3));
        assert_eq!(unread_from_title("(12) Notifications / X"), Some(12));
        assert_eq!(unread_from_title("Home / X"), Some(0));
        assert_eq!(unread_from_title("X. It’s what’s happening / X"), Some(0));
        assert_eq!(unread_from_title("X"), None);
        assert_eq!(unread_from_title(""), None);
        assert_eq!(unread_from_title("X - The Everything App / X"), Some(0));
        assert_eq!(unread_from_title("(lots) Home / X"), None);
    }

    #[test]
    fn titles_lose_their_decoration() {
        assert_eq!(clean_title("(3) Home / X"), "Home");
        assert_eq!(clean_title("Home / X"), "Home");
        assert_eq!(clean_title("Dominikos (@dom) / X"), "Dominikos (@dom)");
        assert_eq!(clean_title("X"), "X");
        assert_eq!(clean_title(""), "");
    }

    #[test]
    fn sections_follow_paths_and_the_known_handle() {
        assert_eq!(section_for("https://x.com/home", None), Section::Home);
        assert_eq!(section_for("https://x.com/", None), Section::Home);
        assert_eq!(section_for("https://x.com/explore", None), Section::Explore);
        assert_eq!(
            section_for("https://x.com/explore/tabs/news", None),
            Section::Explore
        );
        assert_eq!(
            section_for("https://x.com/search?q=rust", None),
            Section::Explore
        );
        assert_eq!(
            section_for("https://x.com/notifications", None),
            Section::Notifications
        );
        assert_eq!(
            section_for("https://x.com/notifications/mentions", None),
            Section::Notifications
        );
        assert_eq!(
            section_for("https://x.com/messages/123", None),
            Section::Messages
        );
        assert_eq!(section_for("https://x.com/i/chat", None), Section::Messages);
        assert_eq!(
            section_for("https://x.com/i/chat/42", None),
            Section::Messages
        );
        assert_eq!(
            section_for("https://x.com/i/bookmarks", None),
            Section::Bookmarks
        );
        assert_eq!(
            section_for("https://x.com/compose/post", None),
            Section::Compose
        );
        assert_eq!(section_for("https://x.com/dom", None), Section::Other);
        assert_eq!(
            section_for("https://x.com/dom", Some("dom")),
            Section::Profile
        );
        assert_eq!(
            section_for("https://x.com/dom/with_replies", Some("dom")),
            Section::Profile
        );
        assert_eq!(
            section_for("https://x.com/someone", Some("dom")),
            Section::Other
        );
        assert_eq!(section_for("not a url", Some("dom")), Section::Other);
    }

    #[test]
    fn destinations_need_a_handle_only_for_profile() {
        assert_eq!(destination_url(Destination::Home, None).expect("ok"), HOME);
        assert!(destination_url(Destination::Profile, None).is_err());
        assert_eq!(
            destination_url(Destination::Profile, Some("dom")).expect("ok"),
            "https://x.com/dom"
        );
    }

    #[test]
    fn handles_are_validated() {
        assert!(valid_handle("dom_314"));
        assert!(valid_handle("a"));
        assert!(!valid_handle(""));
        assert!(!valid_handle("sixteen_chars_xx"));
        assert!(!valid_handle("has space"));
        assert!(!valid_handle("../etc"));
        assert!(!valid_handle("émoji"));
    }

    #[test]
    fn bounds_fill_the_window_inside_the_insets_and_never_go_negative() {
        let insets = Insets {
            left: 236.0,
            top: 44.0,
            right: 0.0,
            bottom: 24.0,
        };
        let (position, size) = bounds_for(insets, LogicalSize::new(1240.0, 820.0));
        assert!((position.x - 236.0).abs() < f64::EPSILON);
        assert!((position.y - 44.0).abs() < f64::EPSILON);
        assert!((size.width - 1004.0).abs() < f64::EPSILON);
        assert!((size.height - 752.0).abs() < f64::EPSILON);

        let (_, tiny) = bounds_for(insets, LogicalSize::new(100.0, 50.0));
        assert!((tiny.width - 1.0).abs() < f64::EPSILON);
        assert!((tiny.height - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn init_script_carries_css_and_niceties_as_json() {
        let script = init_script(
            Niceties::default(),
            &[("dim.css".to_string(), "html{color:red}".to_string())],
        );
        assert!(!script.contains("__TWISTER_CSS__"));
        assert!(!script.contains("__TWISTER_NICETIES__"));
        assert!(!script.contains("__TWISTER_USER_CSS__"));
        assert!(script.contains("[[\"dim.css\",\"html{color:red}\"]]"));
        assert!(script.contains("\"chronologicalHome\":true"));
        assert!(script.contains("data-twister-hide-promoted"));
    }
}
