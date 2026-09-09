//! Preferences and window placement, as two small JSON files in the app data
//! directory. Nothing sensitive lives here: the x.com session is a cookie in
//! the webview's own store, which this app never reads.
//!
//! Every field has a default and unknown fields are ignored, so a file written
//! by a newer or older build still loads — a settings file that refuses to
//! parse must never keep the app from starting.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result, internal};

/// `commands::Settings` in the renderer mirrors this.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// `system` | `light` | `dark`
    pub theme: String,
    /// `off` | `standard` | `strong`
    pub window_material: String,
    pub niceties: Niceties,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "system".into(),
            window_material: "standard".into(),
            niceties: Niceties::default(),
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
    /// The bird in place of the X mark, and the classic blue on the buttons.
    pub classic_bird: bool,
    /// X's retired Dim theme, painted over Lights out.
    pub dim: bool,
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
            classic_bird: false,
            dim: false,
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
