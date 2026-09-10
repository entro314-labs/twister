//! Preferences, window placement and open tabs, as three small JSON files in
//! the app data directory. Nothing sensitive lives here: the x.com session is a cookie in
//! the webview's own store, which this app never reads.
//!
//! Every field has a default and unknown fields are ignored, so a file written
//! by a newer or older build still loads — a settings file that refuses to
//! parse must never keep the app from starting.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result, internal};
use crate::site::SavedTabs;

/// `commands::Settings` in the renderer mirrors this.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// `system` | `light` | `dark`
    pub theme: String,
    /// `off` | `standard` | `strong`
    pub window_material: String,
    pub niceties: Niceties,
    /// A CSS font-family for X's text, or empty for X's own.
    pub font: String,
    /// `small` | `normal` | `large` — the size of a post's text.
    pub text_size: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "system".into(),
            window_material: "standard".into(),
            niceties: Niceties::default(),
            font: String::new(),
            text_size: "normal".into(),
        }
    }
}

/// What the bridge receives: the switches, the font and the text size, as
/// one object.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SitePrefs {
    #[serde(flatten)]
    pub niceties: Niceties,
    pub font: String,
    pub text_size: String,
}

impl Settings {
    pub fn site_prefs(&self) -> SitePrefs {
        SitePrefs {
            niceties: self.niceties,
            font: self.font.clone(),
            text_size: self.text_size.clone(),
        }
    }
}

impl Settings {
    /// Rejects values the renderer could only produce by being wrong.
    pub fn validate(&self) -> Result<()> {
        if !matches!(self.theme.as_str(), "system" | "light" | "dark") {
            return Err(AppError::InvalidInput(format!(
                "Unknown theme `{}`.",
                self.theme
            )));
        }
        if !matches!(self.window_material.as_str(), "off" | "standard" | "strong") {
            return Err(AppError::InvalidInput(format!(
                "Unknown window material `{}`.",
                self.window_material
            )));
        }
        // The font goes into a stylesheet verbatim; a family name has no
        // business carrying anything that could close the declaration.
        if self.font.len() > 120 || self.font.contains([';', '{', '}', '<', '>', '\\', '/']) {
            return Err(AppError::InvalidInput("That is not a font family.".into()));
        }
        if !matches!(self.text_size.as_str(), "small" | "normal" | "large") {
            return Err(AppError::InvalidInput(format!(
                "Unknown text size `{}`.",
                self.text_size
            )));
        }
        Ok(())
    }
}

/// The niceties: what the bridge changes about x.com. Each one is a selector
/// on X's own DOM (see `site/niceties.css` and `site/bridge.js`), which is why
/// each is a switch — a broken one can be turned off without an update.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
// A set of independent switches is exactly what this is.
#[allow(clippy::struct_excessive_bools)]
pub struct Niceties {
    /// Land on "Following" rather than "For you" when opening the home timeline.
    pub chronological_home: bool,
    /// Drop promoted posts from every timeline.
    pub hide_promoted: bool,
    /// Drop the right column: trends, who to follow, premium upsells.
    pub hide_right_column: bool,
    /// Drop Grok, Premium, Jobs and the other non-timeline entries from X's navigation.
    pub hide_extras_nav: bool,
    /// Drop the view counter from each post's action bar.
    pub hide_view_counts: bool,
    /// Drop X's own left navigation entirely — Twister's sidebar covers it.
    /// On by default: two navigation columns is the one thing a wrapper must
    /// not show.
    pub hide_site_nav: bool,
    /// Mirror the unread count from the page title onto the Dock icon.
    pub dock_badge: bool,
    /// Drop the floating Grok and Messages drawers X pins to the bottom-right.
    pub hide_drawers: bool,
    /// Twitter, in one switch: the bird in place of the X mark, the classic
    /// blue on the buttons, and posts called tweets again.
    #[serde(alias = "classicBird")]
    pub classic_twitter: bool,
    /// X's retired Dim theme, painted over Lights out.
    pub dim: bool,
    /// Record the people and posts X loads into the page, for the tools.
    pub capture: bool,
    /// A download button on posts with photos or video.
    pub download_button: bool,
    /// Let the timeline column grow to fill the island.
    pub fit_timeline: bool,
    /// Animated scrolling on keyboard and programmatic jumps.
    pub smooth_scroll: bool,
    /// Posts drawn the way a classic client drew them: a smaller avatar, a
    /// quieter byline, the action bar pulled up under the text.
    pub compact_posts: bool,
    /// Rounded-square avatars in place of circles.
    pub square_avatars: bool,
    /// A post's action bar shows only while the post is hovered or focused.
    pub actions_on_hover: bool,
    /// No reply, repost or like counts on the action bar.
    pub hide_action_counts: bool,
    /// A star in place of the heart, gold when lit.
    pub star_favorites: bool,
    /// A quieter composer: no audience or reply-permission chrome, no Grok,
    /// and a number for the character count in place of X's ring.
    pub compact_compose: bool,
    /// No "What is happening?" box at the top of the home timeline; the
    /// composer is a window of its own, as it was in a client.
    pub hide_inline_composer: bool,
    /// Drop X's sticky page headers — the Home title and its two tabs, the
    /// back arrow and title on a post or a profile. Headers that carry tabs
    /// or a search field elsewhere stay.
    pub hide_page_headers: bool,
    /// Only posts in a timeline: no "Who to follow", "Discover more", news
    /// or premium modules between them.
    pub hide_timeline_modules: bool,
    /// The time on the far right of the byline, the dot before it gone.
    pub time_on_right: bool,
}

impl Default for Niceties {
    fn default() -> Self {
        Self {
            chronological_home: true,
            hide_promoted: true,
            hide_right_column: false,
            hide_extras_nav: true,
            hide_view_counts: false,
            hide_site_nav: true,
            dock_badge: true,
            hide_drawers: false,
            classic_twitter: false,
            dim: false,
            capture: true,
            download_button: true,
            fit_timeline: true,
            smooth_scroll: false,
            compact_posts: false,
            square_avatars: false,
            actions_on_hover: false,
            hide_action_counts: false,
            star_favorites: false,
            compact_compose: false,
            hide_inline_composer: false,
            hide_page_headers: false,
            hide_timeline_modules: false,
            time_on_right: false,
        }
    }
}

/// Where the window was last, in physical pixels. Kept apart from `Settings`
/// because it is not a preference: nothing in the UI reads it.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct WindowBounds {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub maximized: bool,
}

pub fn data_dir() -> Result<PathBuf> {
    let dir = dirs::data_dir()
        .ok_or_else(|| AppError::Internal("No OS data directory.".into()))?
        .join("twister");
    std::fs::create_dir_all(&dir).map_err(|e| internal("Creating the data directory", e))?;
    Ok(dir)
}

pub struct Store {
    dir: PathBuf,
}

impl Store {
    pub fn open() -> Result<Self> {
        Ok(Self { dir: data_dir()? })
    }

    #[cfg(test)]
    pub fn open_at(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// Lenient on purpose: a missing or unreadable file is the defaults, with
    /// the reason logged. Refusing to start over a preference is never right.
    pub fn load_settings(&self) -> Settings {
        load_or_default(&self.dir.join("settings.json"))
    }

    pub fn save_settings(&self, settings: &Settings) -> Result<()> {
        write_atomic(&self.dir.join("settings.json"), settings)
    }

    pub fn load_window(&self) -> Option<WindowBounds> {
        load_or_default::<Option<WindowBounds>>(&self.dir.join("window.json"))
    }

    pub fn save_window(&self, bounds: &WindowBounds) -> Result<()> {
        write_atomic(&self.dir.join("window.json"), bounds)
    }

    pub fn load_tabs(&self) -> SavedTabs {
        load_or_default(&self.dir.join("tabs.json"))
    }

    pub fn save_tabs(&self, tabs: &SavedTabs) -> Result<()> {
        write_atomic(&self.dir.join("tabs.json"), tabs)
    }
}

fn load_or_default<T: for<'de> Deserialize<'de> + Default>(path: &Path) -> T {
    match std::fs::read_to_string(path) {
        Ok(raw) => serde_json::from_str(&raw).unwrap_or_else(|err| {
            log::warn!("{} is unreadable, using defaults: {err}", path.display());
            T::default()
        }),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => T::default(),
        Err(err) => {
            log::warn!("could not read {}: {err}", path.display());
            T::default()
        }
    }
}

/// Write-then-rename, so a crash mid-write leaves the previous file rather
/// than half of the new one.
fn write_atomic<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(value)?)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("twister-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    #[test]
    fn empty_object_is_the_defaults() {
        let parsed: Settings = serde_json::from_str("{}").expect("parses");
        assert_eq!(parsed, Settings::default());
        assert!(parsed.niceties.chronological_home);
        assert!(parsed.niceties.hide_promoted);
        assert!(parsed.niceties.hide_site_nav);
    }

    #[test]
    fn unknown_and_missing_fields_are_tolerated() {
        let parsed: Settings = serde_json::from_str(
            r#"{"theme":"dark","futureField":1,"niceties":{"hidePromoted":false,"alsoNew":true}}"#,
        )
        .expect("parses");
        assert_eq!(parsed.theme, "dark");
        assert!(!parsed.niceties.hide_promoted);
        assert!(parsed.niceties.hide_extras_nav);
        assert!(parsed.niceties.capture);
    }

    #[test]
    fn the_old_bird_switch_still_reads() {
        let parsed: Settings =
            serde_json::from_str(r#"{"niceties":{"classicBird":true}}"#).expect("parses");
        assert!(parsed.niceties.classic_twitter);
        let prefs = serde_json::to_value(parsed.site_prefs()).expect("json");
        assert_eq!(prefs["classicTwitter"], true);
        assert_eq!(prefs["font"], "");
        assert_eq!(prefs["textSize"], "normal");
        assert_eq!(prefs["starFavorites"], false);
    }

    #[test]
    fn the_look_switches_default_off_and_read_back() {
        let parsed: Settings = serde_json::from_str("{}").expect("parses");
        assert!(!parsed.niceties.compact_posts);
        assert!(!parsed.niceties.square_avatars);
        assert!(!parsed.niceties.actions_on_hover);
        assert!(!parsed.niceties.hide_action_counts);
        assert!(!parsed.niceties.star_favorites);
        assert!(!parsed.niceties.compact_compose);
        assert!(!parsed.niceties.hide_inline_composer);
        assert!(!parsed.niceties.hide_page_headers);
        assert!(!parsed.niceties.hide_timeline_modules);
        assert!(!parsed.niceties.time_on_right);
        assert_eq!(parsed.text_size, "normal");
        let parsed: Settings = serde_json::from_str(
            r#"{"textSize":"large","niceties":{"compactPosts":true,"starFavorites":true,"hideTimelineModules":true}}"#,
        )
        .expect("parses");
        assert!(parsed.niceties.compact_posts);
        assert!(parsed.niceties.star_favorites);
        assert!(parsed.niceties.hide_timeline_modules);
        assert!(!parsed.niceties.square_avatars);
        assert_eq!(parsed.text_size, "large");
        assert!(parsed.validate().is_ok());
    }

    #[test]
    fn fonts_are_family_names_only() {
        let mut settings = Settings {
            font: "Inter, sans-serif".into(),
            ..Settings::default()
        };
        assert!(settings.validate().is_ok());
        settings.font = "x; } html { display: none".into();
        assert!(settings.validate().is_err());
        settings.font = "a".repeat(121);
        assert!(settings.validate().is_err());
    }

    #[test]
    fn validation_rejects_unknown_enums() {
        let mut settings = Settings {
            theme: "sepia".into(),
            ..Settings::default()
        };
        assert!(settings.validate().is_err());
        settings.theme = "light".into();
        settings.window_material = "frosted".into();
        assert!(settings.validate().is_err());
        settings.window_material = "off".into();
        assert!(settings.validate().is_ok());
        settings.text_size = "huge".into();
        assert!(settings.validate().is_err());
        settings.text_size = "small".into();
        assert!(settings.validate().is_ok());
    }

    #[test]
    fn settings_round_trip_and_corruption_fall_back() {
        let dir = scratch("settings");
        let store = Store::open_at(&dir);
        assert_eq!(store.load_settings(), Settings::default());

        let settings = Settings {
            theme: "dark".into(),
            niceties: Niceties {
                hide_right_column: true,
                ..Niceties::default()
            },
            ..Settings::default()
        };
        store.save_settings(&settings).expect("saves");
        assert_eq!(store.load_settings(), settings);
        assert!(!dir.join("settings.json.tmp").exists());

        std::fs::write(dir.join("settings.json"), "{not json").expect("writes");
        assert_eq!(store.load_settings(), Settings::default());
    }

    #[test]
    fn window_bounds_round_trip() {
        let dir = scratch("window");
        let store = Store::open_at(&dir);
        assert!(store.load_window().is_none());
        let bounds = WindowBounds {
            x: -10,
            y: 40,
            width: 1240,
            height: 820,
            maximized: false,
        };
        store.save_window(&bounds).expect("saves");
        assert_eq!(store.load_window(), Some(bounds));
    }
}
