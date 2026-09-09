//! User scripts and styles: the Tampermonkey and Stylus of this client.
//!
//! Two folders in the app data directory, `scripts/` and `styles/`. Every
//! `*.js` in the first runs on every x.com page once the DOM is ready, the way
//! a userscript with `@run-at document-idle` would; every `*.css` in the second
//! is applied at document start. Injected as initialization scripts, which the
//! page's content-security policy cannot block — so a script that works in
//! Tampermonkey works here, minus the `GM_*` API.
//!
//! Read once when the site webview is created. Settings offers a reload,
//! which rebuilds the webview rather than trying to un-run a script.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::Result;
use crate::settings::data_dir;

/// One file, as the settings screen lists it.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UserAsset {
    pub name: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UserAssets {
    pub scripts: Vec<UserAsset>,
    pub styles: Vec<UserAsset>,
    pub dir: String,
}

/// The loaded sources, in the order they will be injected.
#[derive(Debug, Clone, Default)]
pub struct Loaded {
    pub scripts: Vec<(String, String)>,
    pub styles: Vec<(String, String)>,
}

pub fn dir() -> Result<PathBuf> {
    let dir = data_dir()?;
    std::fs::create_dir_all(dir.join("scripts"))?;
    std::fs::create_dir_all(dir.join("styles"))?;
    Ok(dir)
}

pub fn load() -> Result<Loaded> {
    let root = dir()?;
    Ok(load_from(&root))
}

pub fn load_from(root: &Path) -> Loaded {
    Loaded {
        scripts: read_all(&root.join("scripts"), "js"),
        styles: read_all(&root.join("styles"), "css"),
    }
}

pub fn list() -> Result<UserAssets> {
    let root = dir()?;
    let loaded = load_from(&root);
    let describe = |entries: &[(String, String)]| {
        entries
            .iter()
            .map(|(name, source)| UserAsset {
                name: name.clone(),
                bytes: source.len() as u64,
            })
            .collect()
    };
    Ok(UserAssets {
        scripts: describe(&loaded.scripts),
        styles: describe(&loaded.styles),
        dir: root.display().to_string(),
    })
}

/// Files sorted by name, so `10-foo.js` runs after `00-bar.js`: the order is
/// the user's to set. Unreadable files are logged and skipped — one bad file
/// must not take the rest down.
fn read_all(folder: &Path, extension: &str) -> Vec<(String, String)> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut files: Vec<(String, String)> = entries
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|e| e.to_str()) == Some(extension))
        .filter_map(|path| {
            let name = path.file_name()?.to_str()?.to_string();
            match std::fs::read_to_string(&path) {
                Ok(source) => Some((name, source)),
                Err(err) => {
                    log::warn!("skipping {}: {err}", path.display());
                    None
                }
            }
        })
        .collect();
    files.sort_by(|a, b| a.0.cmp(&b.0));
    files
}

/// Wraps one user script so it runs once the DOM is ready and a throw inside
/// it lands in the console with the file's name rather than killing the
/// bridge or the next script.
pub fn wrap_script(name: &str, source: &str) -> String {
    let label = serde_json::to_string(name).unwrap_or_else(|_| "\"script\"".into());
    format!(
        "(function(){{\n  var run = function(){{\n    try {{\n{source}\n    }} catch (err) {{ console.error('[twister] user script ' + {label} + ' failed:', err); }}\n  }};\n  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', run, {{ once: true }});\n  else run();\n}})();"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_scripts_and_styles_in_name_order_and_skips_the_rest() {
        let root = std::env::temp_dir().join(format!("twister-userland-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("scripts")).expect("mkdir");
        std::fs::create_dir_all(root.join("styles")).expect("mkdir");
        std::fs::write(root.join("scripts/20-second.js"), "b()").expect("write");
        std::fs::write(root.join("scripts/10-first.js"), "a()").expect("write");
        std::fs::write(root.join("scripts/notes.txt"), "ignored").expect("write");
        std::fs::write(root.join("styles/dim.css"), "html{}").expect("write");

        let loaded = load_from(&root);
        assert_eq!(
            loaded.scripts,
            vec![
                ("10-first.js".to_string(), "a()".to_string()),
                ("20-second.js".to_string(), "b()".to_string())
            ]
        );
        assert_eq!(
            loaded.styles,
            vec![("dim.css".to_string(), "html{}".to_string())]
        );
    }

    #[test]
    fn missing_folders_are_empty_not_errors() {
        let loaded = load_from(Path::new("/definitely/not/here"));
        assert!(loaded.scripts.is_empty());
        assert!(loaded.styles.is_empty());
    }

    #[test]
    fn wrapped_scripts_carry_their_name_and_wait_for_the_dom() {
        let wrapped = wrap_script("hello.js", "console.log(1)");
        assert!(wrapped.contains("\"hello.js\""));
        assert!(wrapped.contains("DOMContentLoaded"));
        assert!(wrapped.contains("console.log(1)"));
    }
}
