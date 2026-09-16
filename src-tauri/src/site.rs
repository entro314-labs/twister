//! The site: a network's page in child webviews — one per tab — and
//! everything the app knows about them. A tab belongs to one network for its
//! life (see `network.rs`); the scripts injected into it are that network's.
//!
//! The shell webview owns the frame — sidebar, titlebar, status bar — and the
//! tabs own the island inside it. A tab's webview sits ON TOP of the shell,
//! so anything the shell needs to show over the island hides the site first
//! (see `set_visible`). Only the active tab is shown; the rest keep loading
//! behind it, sharing the session. Position and size are derived from insets
//! the shell reports, re-applied by Rust on every window resize so the island
//! never lags the frame.
//!
//! Everything that touches a site's DOM is in `site/<network>/*` and
//! `site/common.js`. This file only knows URLs and titles.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use tauri::webview::{NewWindowResponse, PageLoadEvent, WebviewBuilder};
use tauri::{
    AppHandle, Emitter, EventTarget, LogicalPosition, LogicalSize, Manager, Webview, WebviewUrl,
    Window,
};
use url::Url;

use crate::error::{AppError, Result};
use crate::network::{self, Network};
use crate::settings::SitePrefs;
use crate::{ops, userland};

pub const SHELL_LABEL: &str = "shell";
pub const MAX_TABS: usize = 12;

/// The whole site state, pushed to the shell whenever any of it changes.
pub const EVENT_STATE: &str = "twister://site-state";
/// A one-line notice for the status bar. Payload: [`Notice`].
pub const EVENT_NOTICE: &str = "twister://notice";

/// The plumbing every network's scripts share: the response hook, the
/// batcher, the download button, the operations runner.
const COMMON_JS: &str = include_str!("../site/common.js");

/// A network's three scripts and its stylesheet. Each bridge carries the
/// same three placeholders, substituted before injection.
struct Scripts {
    bridge: &'static str,
    capture: &'static str,
    ops: &'static str,
    css: &'static str,
}

fn scripts(network: Network) -> Scripts {
    match network {
        Network::X => Scripts {
            bridge: include_str!("../site/x/bridge.js"),
            capture: include_str!("../site/x/capture.js"),
            ops: include_str!("../site/x/ops.js"),
            css: include_str!("../site/x/niceties.css"),
        },
        Network::Bluesky => Scripts {
            bridge: include_str!("../site/bluesky/bridge.js"),
            capture: include_str!("../site/bluesky/capture.js"),
            ops: include_str!("../site/bluesky/ops.js"),
            css: include_str!("../site/bluesky/niceties.css"),
        },
        Network::Threads => Scripts {
            bridge: include_str!("../site/threads/bridge.js"),
            capture: include_str!("../site/meta/capture.js"),
            ops: include_str!("../site/threads/ops.js"),
            css: include_str!("../site/threads/niceties.css"),
        },
        Network::Instagram => Scripts {
            bridge: include_str!("../site/instagram/bridge.js"),
            capture: include_str!("../site/meta/capture.js"),
            ops: include_str!("../site/instagram/ops.js"),
            css: include_str!("../site/instagram/niceties.css"),
        },
    }
}

// ─── Types ──────────────────────────────────────────────────────────────────

/// One tab, as the shell sees it. Mirrored by `TabState` in the renderer.
#[derive(Debug, Clone, Serialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TabState {
    pub id: u32,
    pub network: Network,
    pub url: String,
    pub section: Section,
    /// The page title with the site's own decoration stripped: "(3) Home / X" → "Home".
    pub title: String,
    /// Parsed from the title; X prefixes it with the unread count.
    pub unread: u32,
    pub loading: bool,
}

/// Mirrored by `SiteState` in the renderer.
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SiteState {
    pub tabs: Vec<TabState>,
    pub active: u32,
    /// The signed-in handle on each network, once its bridge has seen the
    /// site's own profile link. Keyed by the network's slug.
    pub handles: BTreeMap<Network, String>,
}

impl SiteState {
    pub fn active_tab(&self) -> Option<&TabState> {
        self.tabs.iter().find(|t| t.id == self.active)
    }
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

impl Destination {
    pub fn label(self) -> &'static str {
        match self {
            Self::Home => "Home",
            Self::Explore => "Explore",
            Self::Notifications => "Notifications",
            Self::Messages => "Messages",
            Self::Bookmarks => "Bookmarks",
            Self::Profile => "Profile",
            Self::Compose => "Compose",
        }
    }
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
    /// the shell has measured anything. Mirrors `chrome.ts`: the sidebar's
    /// default width, then BOTH top bands — the titlebar the traffic lights
    /// ride in (`TITLEBAR_H`, 52) plus the tab strip under it (`TABBAR_H`, 36)
    /// — and the status bar along the bottom.
    fn default() -> Self {
        Self {
            left: 236.0,
            top: 88.0,
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

/// Open tabs as remembered between launches: URLs, and which was in front.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct SavedTabs {
    pub urls: Vec<String>,
    pub active: usize,
}

struct Tab {
    state: TabState,
    /// The narrowest island this page's layout fits in, as its bridge
    /// measured it; zero until it has.
    min_width: f64,
    /// When this tab was last in front, so switching networks lands on the
    /// tab that was being used there.
    last_active: u64,
}

pub struct Site {
    tabs: Mutex<Vec<Tab>>,
    active: AtomicU32,
    handles: Mutex<BTreeMap<Network, String>>,
    pub insets: Mutex<Insets>,
    /// The last prefs applied, so a fresh page load bakes them in.
    pub prefs: Mutex<SitePrefs>,
    /// Whether the shell wants the site showing. Re-applied after a rebuild.
    pub visible: AtomicBool,
    /// Tab ids only ever go up, so a closed tab's webview label — which the
    /// manager keeps until the close has gone through — is never reused.
    next_id: AtomicU32,
    /// A clock for `last_active`: one tick per activation.
    activations: AtomicU64,
}

impl Site {
    pub fn new(prefs: SitePrefs) -> Self {
        Self {
            tabs: Mutex::new(Vec::new()),
            active: AtomicU32::new(0),
            handles: Mutex::new(BTreeMap::new()),
            insets: Mutex::new(Insets::default()),
            prefs: Mutex::new(prefs),
            visible: AtomicBool::new(true),
            next_id: AtomicU32::new(1),
            activations: AtomicU64::new(1),
        }
    }

    fn snapshot(&self) -> SiteState {
        SiteState {
            tabs: self
                .tabs
                .lock()
                .map(|tabs| tabs.iter().map(|t| t.state.clone()).collect())
                .unwrap_or_default(),
            active: self.active.load(Ordering::Relaxed),
            handles: self.handles.lock().map(|h| h.clone()).unwrap_or_default(),
        }
    }
}

// ─── Pure functions ─────────────────────────────────────────────────────────

pub fn label_for(id: u32) -> String {
    format!("site-{id}")
}

pub fn id_from_label(label: &str) -> Option<u32> {
    label.strip_prefix("site-")?.parse().ok()
}

/// Whether a navigation may happen inside one of `network`'s tabs. Redirect
/// hops each pass through here, so a `t.co` link is allowed and the external
/// page it resolves to is not — which is exactly when it should leave for
/// the browser, or for a tab of the network it belongs to.
pub fn allows(network: Network, url: &Url) -> bool {
    match url.scheme() {
        "about" | "blob" | "data" => return true,
        "http" | "https" => {}
        _ => return false,
    }
    let Some(host) = url.host_str() else {
        return false;
    };
    network.allows_host(host)
}

/// A URL a tab may open on: a network's own site, nothing else — and which
/// network that is.
pub fn tab_url(url: &str) -> Result<(Url, Network)> {
    let parsed = Url::parse(url).map_err(|e| AppError::InvalidInput(format!("Bad URL: {e}")))?;
    let network = network::for_tab_url(&parsed)
        .ok_or_else(|| AppError::InvalidInput(format!("{url} is not somewhere a tab can open.")))?;
    Ok((parsed, network))
}

/// The title with the site's decoration stripped, or `None` when it carries
/// none — a transient title ("X", "") that says nothing about the page.
fn undecorated(network: Network, title: &str) -> Option<&str> {
    let title = title.trim();
    network
        .title_suffixes()
        .iter()
        .find_map(|suffix| title.strip_suffix(suffix))
        .map(str::trim_end)
}

/// The unread count a site puts at the front of every real page title, or
/// `None` when the title is one of the transient ones that carry no
/// information and must not reset the badge.
pub fn unread_from_title(network: Network, title: &str) -> Option<u32> {
    let title = undecorated(network, title)?;
    let Some(rest) = title.strip_prefix('(') else {
        return Some(0);
    };
    let (count, _) = rest.split_once(')')?;
    count.trim().parse().ok()
}

/// "(3) Home / X" → "Home". The Twitter switch rewrites the suffix in the
/// page, so that form is stripped too.
pub fn clean_title(network: Network, title: &str) -> String {
    let mut title = undecorated(network, title).unwrap_or(title.trim());
    if title.starts_with('(')
        && let Some((_, rest)) = title.split_once(')')
    {
        title = rest.trim_start();
    }
    title.to_string()
}

pub fn section_for(network: Network, url: &str, handle: Option<&str>) -> Section {
    let Ok(parsed) = Url::parse(url) else {
        return Section::Other;
    };
    network.section_for(parsed.path(), handle)
}

pub fn destination_url(
    network: Network,
    destination: Destination,
    handle: Option<&str>,
) -> Result<String> {
    Ok(format!(
        "{}{}",
        network.origin(),
        network.destination_path(destination, handle)?
    ))
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

/// Which tab to show once `closing` is gone: the one after it, else the one
/// before. `None` when it was the last.
pub fn neighbour(ids: &[u32], closing: u32) -> Option<u32> {
    let index = ids.iter().position(|&id| id == closing)?;
    ids.get(index + 1)
        .or_else(|| index.checked_sub(1).and_then(|i| ids.get(i)))
        .copied()
}

fn init_script(network: Network, prefs: &SitePrefs, user_styles: &[(String, String)]) -> String {
    let scripts = scripts(network);
    // Every substitution is JSON, which is valid JavaScript for a string, an
    // array and an object literal alike.
    scripts
        .bridge
        .replace(
            "__TWISTER_CSS__",
            &serde_json::to_string(scripts.css).unwrap_or_else(|_| "\"\"".into()),
        )
        .replace(
            "__TWISTER_USER_CSS__",
            &serde_json::to_string(user_styles).unwrap_or_else(|_| "[]".into()),
        )
        .replace(
            "__TWISTER_NICETIES__",
            &serde_json::to_string(prefs).unwrap_or_else(|_| "{}".into()),
        )
}

// ─── Wiring ─────────────────────────────────────────────────────────────────

fn site(app: &AppHandle) -> tauri::State<'_, crate::commands::AppState> {
    app.state::<crate::commands::AppState>()
}

fn prefs(app: &AppHandle) -> SitePrefs {
    site(app).site.prefs.lock().map_or_else(
        |_| SitePrefs {
            niceties: crate::settings::Niceties::default(),
            font: String::new(),
            text_size: "normal".into(),
        },
        |p| p.clone(),
    )
}

/// Opens a tab on `url` as a child webview of the window. Bridge commands and
/// events reach the state through the app handle, which is why the [`Site`]
/// must already be managed when this runs.
// The builder's callbacks are the length; splitting them out would only hide
// which tab each one speaks for.
#[allow(clippy::too_many_lines)]
pub fn open_tab(app: &AppHandle, window: &Window, url: &str, activate: bool) -> Result<u32> {
    let (start, network) = tab_url(url)?;
    let state = site(app);
    let count = state.site.tabs.lock().map_or(0, |t| t.len());
    if count >= MAX_TABS {
        return Err(AppError::InvalidInput(format!(
            "{MAX_TABS} tabs is plenty. Close one first."
        )));
    }
    let id = state.site.next_id.fetch_add(1, Ordering::Relaxed);
    let label = label_for(id);

    let scale = window.scale_factor()?;
    let size: LogicalSize<f64> = window.inner_size()?.to_logical(scale);
    let insets = state
        .site
        .insets
        .lock()
        .map_or_else(|_| Insets::default(), |i| *i);
    let (position, bounds) = bounds_for(insets, size);

    // The user's own scripts and styles, read fresh on every build: a reload
    // from Settings is a rebuild, and this is where it picks the changes up.
    let assets = userland::load().unwrap_or_else(|err| {
        log::warn!("user scripts and styles unavailable: {err}");
        userland::Loaded::default()
    });
    let current_prefs = prefs(app);

    let nav_handle = app.clone();
    let new_window_handle = app.clone();
    let title_handle = app.clone();
    let load_handle = app.clone();

    let mut builder = WebviewBuilder::new(&label, WebviewUrl::External(start.clone()))
        .initialization_script(COMMON_JS)
        .initialization_script(init_script(network, &current_prefs, &assets.styles))
        .initialization_script(scripts(network).capture)
        .initialization_script(scripts(network).ops);
    for (name, source) in &assets.scripts {
        builder = builder.initialization_script(userland::wrap_script(name, source));
    }
    let builder = builder
        .devtools(cfg!(debug_assertions))
        .on_navigation(move |url| {
            if allows(network, url) {
                return true;
            }
            // A link from one network to another's page is a tab there,
            // with that network's scripts; anything else leaves.
            leave(&nav_handle, url);
            false
        })
        .on_new_window(move |url, _features| {
            // A popup is either a page of this network — a new tab — or a
            // link out. Either way no second window: a client is one frame.
            if allows(network, &url) {
                if let Err(err) = new_tab(&new_window_handle, Some(url.to_string()), None) {
                    log::warn!("could not open {url} in a tab: {err}");
                }
            } else {
                leave(&new_window_handle, &url);
            }
            NewWindowResponse::Deny
        })
        .on_document_title_changed(move |webview, title| {
            let Some(id) = id_from_label(webview.label()) else {
                return;
            };
            update_tab(&title_handle, id, |tab| {
                let cleaned = clean_title(network, &title);
                if !cleaned.is_empty() {
                    tab.title = cleaned;
                }
                if let Some(unread) = unread_from_title(network, &title) {
                    tab.unread = unread;
                }
            });
        })
        .on_page_load(move |webview, payload| {
            let Some(id) = id_from_label(webview.label()) else {
                return;
            };
            let url = payload.url().to_string();
            let loading = matches!(payload.event(), PageLoadEvent::Started);
            log::debug!(
                "tab {id} load {}: {url}",
                if loading { "started" } else { "finished" }
            );
            if loading && active_id(&load_handle) == Some(id) {
                ops::page_reloaded(&load_handle);
            }
            let handle = current_handle(&load_handle, network);
            update_tab(&load_handle, id, |tab| {
                tab.section = section_for(network, &url, handle.as_deref());
                tab.url = url;
                tab.loading = loading;
            });
        });

    let webview = window.add_child(builder, position, bounds)?;
    {
        let mut tabs = state
            .site
            .tabs
            .lock()
            .map_err(|_| AppError::Internal("Tabs lock poisoned.".into()))?;
        tabs.push(Tab {
            state: TabState {
                id,
                network,
                url: start.to_string(),
                section: section_for(network, start.as_str(), None),
                loading: true,
                ..TabState::default()
            },
            min_width: 0.0,
            last_active: 0,
        });
    }
    if activate || count == 0 {
        activate_tab(app, id)?;
    } else {
        let _ = webview.hide();
        push(app);
    }
    Ok(id)
}

/// A tab on `url`, or on `network`'s home, or on the front tab's network's
/// home — the way ⌘T opens beside what is being read.
pub fn new_tab(app: &AppHandle, url: Option<String>, network: Option<Network>) -> Result<u32> {
    let window = main_window(app)?;
    let url = url.unwrap_or_else(|| {
        network
            .or_else(|| active_network(app))
            .unwrap_or_default()
            .home()
    });
    open_tab(app, &window, &url, true)
}

/// Brings a network to the front: the tab that was last used there, or a
/// new one on its home.
pub fn activate_network(app: &AppHandle, network: Network) -> Result<u32> {
    let recent = site(app).site.tabs.lock().ok().and_then(|tabs| {
        tabs.iter()
            .filter(|t| t.state.network == network)
            .max_by_key(|t| t.last_active)
            .map(|t| t.state.id)
    });
    match recent {
        Some(id) => {
            activate_tab(app, id)?;
            Ok(id)
        }
        None => new_tab(app, None, Some(network)),
    }
}

pub fn activate_tab(app: &AppHandle, id: u32) -> Result<()> {
    let state = site(app);
    let ids = tab_ids(app);
    if !ids.contains(&id) {
        return Err(AppError::NotFound("That tab is gone.".into()));
    }
    let previous = state.site.active.swap(id, Ordering::Relaxed);
    let tick = state.site.activations.fetch_add(1, Ordering::Relaxed);
    if let Ok(mut tabs) = state.site.tabs.lock()
        && let Some(tab) = tabs.iter_mut().find(|t| t.state.id == id)
    {
        tab.last_active = tick;
    }
    let visible = state.site.visible.load(Ordering::Relaxed);
    if previous != id
        && let Some(old) = app.get_webview(&label_for(previous))
    {
        let _ = old.hide();
    }
    if let Some(webview) = app.get_webview(&label_for(id)) {
        if visible {
            webview.show()?;
            let _ = webview.set_focus();
        } else {
            let _ = webview.hide();
        }
    }
    push(app);
    // The window's floor follows the page in front.
    if let Err(err) = fit_window(app) {
        log::debug!("fit skipped: {err}");
    }
    Ok(())
}

/// Closes a tab. The last tab closing hides the window, the way a browser
/// window with one tab does — the session stays warm.
pub fn close_tab(app: &AppHandle, id: u32) -> Result<()> {
    let ids = tab_ids(app);
    if !ids.contains(&id) {
        return Err(AppError::NotFound("That tab is gone.".into()));
    }
    if ids.len() == 1 {
        let window = main_window(app)?;
        crate::remember_bounds(app);
        window.hide()?;
        return Ok(());
    }
    if active_id(app) == Some(id)
        && let Some(next) = neighbour(&ids, id)
    {
        activate_tab(app, next)?;
    }
    if let Ok(mut tabs) = site(app).site.tabs.lock() {
        tabs.retain(|t| t.state.id != id);
    }
    if let Some(webview) = app.get_webview(&label_for(id)) {
        webview.close()?;
    }
    push(app);
    Ok(())
}

/// The tab `delta` places along, wrapping.
pub fn step_tab(app: &AppHandle, delta: i32) -> Result<()> {
    let ids = tab_ids(app);
    let Some(active) = active_id(app) else {
        return Ok(());
    };
    let Some(index) = ids.iter().position(|&id| id == active) else {
        return Ok(());
    };
    let len = i32::try_from(ids.len()).unwrap_or(1);
    let next = (i32::try_from(index).unwrap_or(0) + delta).rem_euclid(len);
    let next = ids
        .get(usize::try_from(next).unwrap_or(0))
        .copied()
        .unwrap_or(active);
    activate_tab(app, next)
}

/// Closes every tab and opens them again, which is the only way to change
/// their initialization scripts — and therefore how a user script or style
/// added to the folder starts running.
pub fn rebuild(app: &AppHandle) -> Result<()> {
    let window = main_window(app)?;
    let saved = saved_tabs(app);
    for id in tab_ids(app) {
        if let Some(webview) = app.get_webview(&label_for(id)) {
            webview.close()?;
        }
    }
    if let Ok(mut tabs) = site(app).site.tabs.lock() {
        tabs.clear();
    }
    restore_tabs(app, &window, &saved)?;
    Ok(())
}

/// Opens the remembered tabs, or Home when there are none.
pub fn restore_tabs(app: &AppHandle, window: &Window, saved: &SavedTabs) -> Result<()> {
    let urls: Vec<&str> = saved
        .urls
        .iter()
        .map(String::as_str)
        .filter(|u| tab_url(u).is_ok())
        .take(MAX_TABS)
        .collect();
    if urls.is_empty() {
        open_tab(app, window, &Network::default().home(), true)?;
        return Ok(());
    }
    let active = saved.active.min(urls.len() - 1);
    let mut active_id = None;
    for (index, url) in urls.iter().enumerate() {
        let id = open_tab(app, window, url, false)?;
        if index == active {
            active_id = Some(id);
        }
    }
    if let Some(id) = active_id {
        activate_tab(app, id)?;
    }
    Ok(())
}

pub fn saved_tabs(app: &AppHandle) -> SavedTabs {
    let snapshot = site(app).site.snapshot();
    let active = snapshot
        .tabs
        .iter()
        .position(|t| t.id == snapshot.active)
        .unwrap_or(0);
    SavedTabs {
        urls: snapshot.tabs.into_iter().map(|t| t.url).collect(),
        active,
    }
}

fn main_window(app: &AppHandle) -> Result<Window> {
    app.get_window(crate::MAIN_WINDOW)
        .ok_or_else(|| AppError::NotFound("The main window is gone.".into()))
}

fn tab_ids(app: &AppHandle) -> Vec<u32> {
    site(app)
        .site
        .tabs
        .lock()
        .map(|tabs| tabs.iter().map(|t| t.state.id).collect())
        .unwrap_or_default()
}

fn active_id(app: &AppHandle) -> Option<u32> {
    let id = site(app).site.active.load(Ordering::Relaxed);
    (id != 0).then_some(id)
}

fn current_handle(app: &AppHandle, network: Network) -> Option<String> {
    site(app)
        .site
        .handles
        .lock()
        .ok()
        .and_then(|h| h.get(&network).cloned())
}

/// The signed-in handle on a network, if its bridge has seen one.
pub fn handle_on(app: &AppHandle, network: Network) -> Option<String> {
    current_handle(app, network)
}

fn tab_network(app: &AppHandle, id: u32) -> Option<Network> {
    site(app).site.tabs.lock().ok().and_then(|tabs| {
        tabs.iter()
            .find(|t| t.state.id == id)
            .map(|t| t.state.network)
    })
}

/// The front tab's network, if there is a tab.
pub fn active_network(app: &AppHandle) -> Option<Network> {
    active_id(app).and_then(|id| tab_network(app, id))
}

/// Which network a webview label speaks for, for checking who is calling.
pub fn network_of_label(app: &AppHandle, label: &str) -> Option<Network> {
    id_from_label(label).and_then(|id| tab_network(app, id))
}

/// Which network's page a webview is showing right now, for what it sends
/// the store. A Threads tab signs in through instagram.com, and its
/// allowlist lets it stay there; what that page loads is Instagram's, not
/// Threads', and is filed as such. Falls back to the tab's own network for a
/// host that is not a tab host — a sign-in provider's, say.
pub fn page_network_of_label(app: &AppHandle, label: &str) -> Option<Network> {
    let id = id_from_label(label)?;
    let state = site(app);
    let tabs = state.site.tabs.lock().ok()?;
    let tab = tabs.iter().find(|t| t.state.id == id)?;
    Some(
        Url::parse(&tab.state.url)
            .ok()
            .and_then(|url| network::for_tab_url(&url))
            .unwrap_or(tab.state.network),
    )
}

/// The active tab's webview label, for checking who is calling.
pub fn active_label(app: &AppHandle) -> Option<String> {
    active_id(app).map(label_for)
}

/// The active tab's webview.
fn webview(app: &AppHandle) -> Result<Webview> {
    let id = active_id(app).ok_or_else(|| AppError::NotFound("No tab is open.".into()))?;
    app.get_webview(&label_for(id))
        .ok_or_else(|| AppError::NotFound("The site webview is gone.".into()))
}

/// What the operations and the scheduler need to know about the front tab.
pub struct Snapshot {
    pub url: String,
    pub loading: bool,
}

pub fn current_state(app: &AppHandle) -> Option<Snapshot> {
    let snapshot = site(app).site.snapshot();
    let tab = snapshot.active_tab()?;
    Some(Snapshot {
        url: tab.url.clone(),
        loading: tab.loading,
    })
}

pub fn state(app: &AppHandle) -> SiteState {
    site(app).site.snapshot()
}

/// Runs a script of ours in the active tab. This is how the shell reaches
/// the bridge (`__twister`) and the operations (`__twisterOps`); every
/// argument is JSON-encoded by the caller before it goes in.
pub fn eval(app: &AppHandle, script: &str) -> Result<()> {
    webview(app)?.eval(script)?;
    Ok(())
}

/// Mutates one tab's state and pushes the result to the shell.
fn update_tab(app: &AppHandle, id: u32, change: impl FnOnce(&mut TabState)) {
    {
        let state = site(app);
        let Ok(mut tabs) = state.site.tabs.lock() else {
            return;
        };
        let Some(tab) = tabs.iter_mut().find(|t| t.state.id == id) else {
            return;
        };
        change(&mut tab.state);
    }
    push(app);
}

/// Pushes the whole state to the shell and syncs the badge. Every change
/// goes through here so the shell and the Dock badge can never disagree.
fn push(app: &AppHandle) {
    let snapshot = site(app).site.snapshot();
    let dock_badge = prefs(app).niceties.dock_badge;
    let unread = snapshot.active_tab().map_or(0, |t| t.unread);
    sync_badge(app, unread, dock_badge);
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

/// A URL that is not this tab's to show: a tab of the network it belongs to
/// when it is one of theirs, the system browser otherwise.
fn leave(app: &AppHandle, url: &Url) {
    if network::for_tab_url(url).is_some() {
        match new_tab(app, Some(url.to_string()), None) {
            Ok(_) => return,
            Err(err) => log::warn!("could not open {url} in a tab: {err}"),
        }
    }
    open_external(app, url);
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

/// A full load of `url` in the active tab.
pub fn navigate(app: &AppHandle, url: &str) -> Result<()> {
    let parsed = Url::parse(url).map_err(|e| AppError::InvalidInput(format!("Bad URL: {e}")))?;
    let network =
        active_network(app).ok_or_else(|| AppError::NotFound("No tab is open.".into()))?;
    if !allows(network, &parsed) {
        return Err(AppError::InvalidInput(format!(
            "{url} is not somewhere the site can go."
        )));
    }
    webview(app)?.navigate(parsed)?;
    Ok(())
}

/// Sends the active tab to a destination through the site's own navigation
/// when the bridge is there to click it, and by a full load otherwise. The
/// distinction is what keeps a sidebar click instant — and what makes X's
/// compose modal open at all: loaded cold, `/compose/post` never gets past
/// X's splash.
pub fn go(app: &AppHandle, destination: Destination) -> Result<()> {
    let network =
        active_network(app).ok_or_else(|| AppError::NotFound("No tab is open.".into()))?;
    let handle = current_handle(app, network);
    let url = destination_url(network, destination, handle.as_deref())?;
    let parsed = Url::parse(&url).map_err(|e| AppError::Internal(e.to_string()))?;
    let path = serde_json::to_string(parsed.path())?;
    let href = serde_json::to_string(&url)?;
    eval(
        app,
        &format!("(window.__twister && window.__twister.go({path})) || location.assign({href})"),
    )
}

/// Sends the active tab to a path on its own site through the bridge, in-app.
pub fn go_path(app: &AppHandle, path: &str) -> Result<()> {
    if !path.starts_with('/') || path.contains("//") {
        return Err(AppError::InvalidInput("That is not a site path.".into()));
    }
    let json = serde_json::to_string(path)?;
    eval(
        app,
        &format!("window.__twister && window.__twister.go({json})"),
    )
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
/// stored insets, for every tab. Called on every resize and whenever the
/// insets change.
pub fn layout(app: &AppHandle) -> Result<()> {
    let window = main_window(app)?;
    let insets = site(app)
        .site
        .insets
        .lock()
        .map(|insets| *insets)
        .unwrap_or_default();
    let scale = window.scale_factor()?;
    let size: LogicalSize<f64> = window.inner_size()?.to_logical(scale);
    let (position, bounds) = bounds_for(insets, size);
    for id in tab_ids(app) {
        if let Some(webview) = app.get_webview(&label_for(id)) {
            webview.set_position(position)?;
            webview.set_size(bounds)?;
        }
    }
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
        fit_window(app)?;
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

/// Pushes new prefs into every live page and remembers them for the next load.
pub fn apply_prefs(app: &AppHandle, prefs: &SitePrefs) -> Result<()> {
    if let Ok(mut current) = site(app).site.prefs.lock() {
        *current = prefs.clone();
    }
    let json = serde_json::to_string(prefs)?;
    for id in tab_ids(app) {
        if let Some(webview) = app.get_webview(&label_for(id)) {
            webview.eval(format!(
                "window.__twister && window.__twister.apply({json})"
            ))?;
        }
    }
    push(app);
    Ok(())
}

/// Forgets every session: the tabs share one data store, so every cookie
/// and every byte of site storage for every network goes, then back to the
/// front door of the front tab's network in one tab.
pub fn sign_out(app: &AppHandle) -> Result<()> {
    let webview = webview(app)?;
    let network = active_network(app).unwrap_or_default();
    webview.clear_all_browsing_data()?;
    if let Ok(mut handles) = site(app).site.handles.lock() {
        handles.clear();
    }
    let active = active_id(app);
    for id in tab_ids(app) {
        if Some(id) != active {
            close_tab(app, id)?;
        }
    }
    webview
        .navigate(Url::parse(&network.home()).map_err(|e| AppError::Internal(e.to_string()))?)?;
    push(app);
    Ok(())
}

/// The window has to be wide enough for the front page's own layout plus the
/// frame around it. Applied as the minimum size, and as the size when the
/// window is narrower than that right now — showing X's right column again
/// must not leave it cut off.
pub fn fit_window(app: &AppHandle) -> Result<()> {
    let window = main_window(app)?;
    let content = active_id(app)
        .and_then(|active| {
            site(app).site.tabs.lock().ok().and_then(|tabs| {
                tabs.iter()
                    .find(|t| t.state.id == active)
                    .map(|t| t.min_width)
            })
        })
        .unwrap_or(0.0);
    if content <= 0.0 {
        return Ok(());
    }
    let insets = site(app)
        .site
        .insets
        .lock()
        .map(|insets| *insets)
        .unwrap_or_default();
    let min_width = (insets.left + content + insets.right).max(crate::MIN_SIZE.0);
    window.set_min_size(Some(LogicalSize::new(min_width, crate::MIN_SIZE.1)))?;
    let scale = window.scale_factor()?;
    let size: LogicalSize<f64> = window.inner_size()?.to_logical(scale);
    if size.width < min_width && !window.is_maximized().unwrap_or(false) {
        window.set_size(LogicalSize::new(min_width, size.height))?;
    }
    Ok(())
}

// ─── Bridge ─────────────────────────────────────────────────────────────────
// Called from the page. Inputs are untrusted; the caller's webview says
// which tab is talking.

pub fn bridge_navigated(app: &AppHandle, caller: &str, url: &str) {
    let Some(id) = id_from_label(caller) else {
        return;
    };
    let Ok(parsed) = Url::parse(url) else {
        return;
    };
    let Some(network) = tab_network(app, id) else {
        return;
    };
    if !allows(network, &parsed) {
        return;
    }
    log::debug!("tab {id} in-app navigation: {url}");
    let handle = current_handle(app, network);
    update_tab(app, id, |tab| {
        tab.section = section_for(network, url, handle.as_deref());
        tab.url = url.to_string();
    });
}

/// The page found the signed-in handle. It counts for the calling tab's
/// network only, and is checked against that network's rule.
pub fn bridge_profile(app: &AppHandle, caller: &str, handle: &str) -> Result<()> {
    let network = network_of_label(app, caller)
        .ok_or_else(|| AppError::NotFound("That tab is gone.".into()))?;
    if !network.valid_handle(handle) {
        return Err(AppError::InvalidInput(format!(
            "That is not a {} handle.",
            network.name()
        )));
    }
    if let Ok(mut handles) = site(app).site.handles.lock() {
        handles.insert(network, handle.to_string());
    }
    if let Ok(mut tabs) = site(app).site.tabs.lock() {
        for tab in tabs.iter_mut().filter(|t| t.state.network == network) {
            tab.state.section = section_for(network, &tab.state.url, Some(handle));
        }
    }
    push(app);
    Ok(())
}

/// The bridge measured how wide its page's layout wants to be in this tab.
pub fn bridge_layout(app: &AppHandle, caller: &str, min_width: f64) -> Result<()> {
    if !min_width.is_finite() || !(0.0..=4000.0).contains(&min_width) {
        return Err(AppError::InvalidInput("That is not a width.".into()));
    }
    let Some(id) = id_from_label(caller) else {
        return Ok(());
    };
    let changed = {
        let state = site(app);
        let mut tabs = state
            .site
            .tabs
            .lock()
            .map_err(|_| AppError::Internal("Tabs lock poisoned.".into()))?;
        let Some(tab) = tabs.iter_mut().find(|t| t.state.id == id) else {
            return Ok(());
        };
        let changed = (tab.min_width - min_width).abs() > 0.5;
        tab.min_width = min_width;
        changed
    };
    if changed && Some(id) == active_id(app) {
        log::debug!("tab {caller} wants {min_width}px of island");
        fit_window(app)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(s: &str) -> Url {
        Url::parse(s).expect("test url")
    }

    #[test]
    fn allowlist_covers_the_networks_site_and_its_sign_in_frames_only() {
        assert!(allows(Network::X, &url("https://x.com/home")));
        assert!(allows(Network::X, &url("https://twitter.com/home")));
        assert!(allows(Network::X, &url("https://api.x.com/graphql")));
        assert!(allows(
            Network::X,
            &url("https://pbs.twimg.com/media/a.jpg")
        ));
        assert!(allows(Network::X, &url("https://t.co/abc")));
        assert!(allows(
            Network::X,
            &url("https://accounts.google.com/gsi/button")
        ));
        assert!(allows(Network::X, &url("about:blank")));
        assert!(allows(Network::X, &url("about:srcdoc")));
        assert!(!allows(Network::X, &url("https://example.com/")));
        assert!(!allows(Network::X, &url("https://notx.com/")));
        assert!(!allows(Network::X, &url("https://x.com.evil.example/")));
        assert!(!allows(Network::X, &url("https://google.com/")));
        assert!(!allows(Network::X, &url("mailto:a@b.c")));
        assert!(!allows(Network::X, &url("ftp://x.com/")));
        // A Bluesky link inside an X tab leaves the tab — for a Bluesky tab.
        assert!(!allows(
            Network::X,
            &url("https://bsky.app/profile/bsky.app")
        ));
        assert!(allows(
            Network::Bluesky,
            &url("https://bsky.app/profile/bsky.app")
        ));
    }

    #[test]
    fn tabs_open_on_a_networks_site_only() {
        assert_eq!(
            tab_url("https://x.com/i/bookmarks").expect("ok").1,
            Network::X
        );
        assert_eq!(
            tab_url("https://twitter.com/home").expect("ok").1,
            Network::X
        );
        assert_eq!(
            tab_url("https://bsky.app/notifications").expect("ok").1,
            Network::Bluesky
        );
        assert_eq!(
            tab_url("https://www.threads.com/").expect("ok").1,
            Network::Threads
        );
        assert_eq!(
            tab_url("https://www.instagram.com/direct/inbox/")
                .expect("ok")
                .1,
            Network::Instagram
        );
        assert!(tab_url("https://t.co/abc").is_err());
        assert!(tab_url("http://x.com/home").is_err());
        assert!(tab_url("https://pbs.twimg.com/a.jpg").is_err());
        assert!(tab_url("nope").is_err());
    }

    #[test]
    fn labels_carry_the_tab_id() {
        assert_eq!(label_for(7), "site-7");
        assert_eq!(id_from_label("site-7"), Some(7));
        assert_eq!(id_from_label("shell"), None);
        assert_eq!(id_from_label("site-x"), None);
    }

    #[test]
    fn closing_a_tab_moves_to_its_neighbour() {
        assert_eq!(neighbour(&[1, 2, 3], 2), Some(3));
        assert_eq!(neighbour(&[1, 2, 3], 3), Some(2));
        assert_eq!(neighbour(&[1, 2, 3], 1), Some(2));
        assert_eq!(neighbour(&[1], 1), None);
        assert_eq!(neighbour(&[1, 2], 9), None);
    }

    #[test]
    fn unread_is_read_from_real_titles_only() {
        let x = Network::X;
        assert_eq!(unread_from_title(x, "(3) Home / X"), Some(3));
        assert_eq!(unread_from_title(x, "(12) Notifications / X"), Some(12));
        assert_eq!(unread_from_title(x, "Home / X"), Some(0));
        assert_eq!(
            unread_from_title(x, "X. It’s what’s happening / X"),
            Some(0)
        );
        assert_eq!(unread_from_title(x, "X"), None);
        assert_eq!(unread_from_title(x, ""), None);
        assert_eq!(unread_from_title(x, "X - The Everything App / X"), Some(0));
        assert_eq!(unread_from_title(x, "(lots) Home / X"), None);
        assert_eq!(
            unread_from_title(Network::Bluesky, "(2) Notifications — Bluesky"),
            Some(2)
        );
        assert_eq!(unread_from_title(Network::Bluesky, "Bluesky"), None);
        assert_eq!(
            unread_from_title(
                Network::Threads,
                "Mark Zuckerberg (@zuck) • Threads, Say more"
            ),
            Some(0)
        );
    }

    #[test]
    fn titles_lose_their_decoration() {
        let x = Network::X;
        assert_eq!(clean_title(x, "(3) Home / X"), "Home");
        assert_eq!(clean_title(x, "Home / X"), "Home");
        assert_eq!(clean_title(x, "(2) Home / Twitter"), "Home");
        assert_eq!(clean_title(x, "Dominikos (@dom) / X"), "Dominikos (@dom)");
        assert_eq!(clean_title(x, "X"), "X");
        assert_eq!(clean_title(x, ""), "");
        assert_eq!(
            clean_title(Network::Bluesky, "Bluesky (@bsky.app) — Bluesky"),
            "Bluesky (@bsky.app)"
        );
        assert_eq!(
            clean_title(
                Network::Threads,
                "Mark Zuckerberg (@zuck) • Threads, Say more"
            ),
            "Mark Zuckerberg (@zuck)"
        );
        assert_eq!(
            clean_title(
                Network::Instagram,
                "Instagram (@instagram) • Instagram photos and videos"
            ),
            "Instagram (@instagram)"
        );
        assert_eq!(clean_title(Network::Instagram, "Instagram"), "Instagram");
    }

    #[test]
    fn sections_come_from_the_network_and_the_url() {
        assert_eq!(
            section_for(Network::X, "https://x.com/home", None),
            Section::Home
        );
        assert_eq!(
            section_for(Network::X, "https://x.com/dom", Some("dom")),
            Section::Profile
        );
        assert_eq!(
            section_for(Network::Bluesky, "https://bsky.app/saved", None),
            Section::Bookmarks
        );
        assert_eq!(
            section_for(Network::X, "not a url", Some("dom")),
            Section::Other
        );
    }

    #[test]
    fn destinations_are_full_urls_on_the_networks_origin() {
        assert_eq!(
            destination_url(Network::X, Destination::Home, None).expect("ok"),
            "https://x.com/home"
        );
        assert!(destination_url(Network::X, Destination::Profile, None).is_err());
        assert_eq!(
            destination_url(Network::X, Destination::Profile, Some("dom")).expect("ok"),
            "https://x.com/dom"
        );
        assert_eq!(
            destination_url(Network::Bluesky, Destination::Compose, None).expect("ok"),
            "https://bsky.app/intent/compose"
        );
        assert!(destination_url(Network::Instagram, Destination::Compose, None).is_err());
    }

    #[test]
    fn bounds_fill_the_window_inside_the_insets_and_never_go_negative() {
        let insets = Insets {
            left: 236.0,
            top: 88.0,
            right: 0.0,
            bottom: 24.0,
        };
        let (position, size) = bounds_for(insets, LogicalSize::new(1240.0, 820.0));
        assert!((position.x - 236.0).abs() < f64::EPSILON);
        assert!((position.y - 88.0).abs() < f64::EPSILON);
        assert!((size.width - 1004.0).abs() < f64::EPSILON);
        assert!((size.height - 708.0).abs() < f64::EPSILON);

        let (_, tiny) = bounds_for(insets, LogicalSize::new(100.0, 50.0));
        assert!((tiny.width - 1.0).abs() < f64::EPSILON);
        assert!((tiny.height - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn init_script_carries_css_and_prefs_as_json_for_every_network() {
        let prefs = SitePrefs {
            niceties: crate::settings::Niceties::default(),
            font: "Inter".into(),
            text_size: "normal".into(),
        };
        for network in crate::network::ALL {
            let script = init_script(
                *network,
                &prefs,
                &[("dim.css".to_string(), "html{color:red}".to_string())],
            );
            assert!(!script.contains("__TWISTER_CSS__"), "{network:?}");
            assert!(!script.contains("__TWISTER_NICETIES__"), "{network:?}");
            assert!(!script.contains("__TWISTER_USER_CSS__"), "{network:?}");
            assert!(script.contains("[[\"dim.css\",\"html{color:red}\"]]"));
            assert!(script.contains("\"chronologicalHome\":true"));
            assert!(script.contains("\"font\":\"Inter\""));
        }
        assert!(init_script(Network::X, &prefs, &[]).contains("data-twister-hide-promoted"));
    }

    #[test]
    fn site_state_keys_handles_by_slug() {
        let mut handles = BTreeMap::new();
        handles.insert(Network::Bluesky, "a.bsky.social".to_string());
        let state = SiteState {
            handles,
            ..SiteState::default()
        };
        let json = serde_json::to_value(&state).expect("json");
        assert_eq!(json["handles"]["bluesky"], "a.bsky.social");
        assert!(json["handles"]["x"].is_null());
    }

    #[test]
    fn saved_tabs_default_to_nothing() {
        let saved: SavedTabs = serde_json::from_str("{}").expect("parses");
        assert!(saved.urls.is_empty());
        assert_eq!(saved.active, 0);
    }
}
