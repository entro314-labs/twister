//! Deferred, multi-channel auto-updates.
//!
//! ## Deferred install
//!
//! [`stage_update`] downloads and signature-verifies the bundle into managed
//! state but does NOT install it: replacing the bundle of a live process breaks
//! the running app's code signature. The staged bytes are applied from
//! `RunEvent::ExitRequested` (see [`install_pending_on_exit`]) or explicitly by
//! [`restart_and_install`], the Settings "Restart now" button.
//!
//! Note that Twister's window CLOSES to a hidden window rather than quitting —
//! `ExitRequested` fires on ⌘Q and on Quit from the menu, which is exactly when
//! nothing is executing out of the bundle.
//!
//! ## A dead pipeline is not "no release yet"
//!
//! The updater plugin reports both "this channel has never published" and "the
//! releases repository is missing, private or renamed" as `ReleaseNotFound`,
//! which would let a permanently broken update pipeline read to the user as the
//! calm pre-first-release state forever. When the manifest misses,
//! [`check`] probes the releases repository itself and answers
//! [`AppError::UpdateSourceUnreachable`] when the repo does not answer.

use std::sync::Mutex;

use serde::Serialize;
use tauri::{Emitter, Manager};
use tauri_plugin_updater::UpdaterExt;

use crate::error::{AppError, Result};

/// A downloaded-and-verified update waiting for exit to install.
#[derive(Default)]
pub struct PendingUpdate(pub Mutex<Option<(tauri_plugin_updater::Update, Vec<u8>)>>);

/// The public releases repository every channel manifest is an asset of. The
/// unreachable probe interrogates this host, so the
/// `endpoints_live_on_the_releases_repo` test keeps it and [`Channel::endpoint`]
/// from drifting apart.
const RELEASES_REPO_URL: &str = "https://github.com/entro314-labs/twister-releases";

/// Live download progress. Payload: `{ downloaded, total }`, `total` null when
/// the release server sent no `Content-Length`.
pub const EVENT_PROGRESS: &str = "twister://update-progress";

/// At most ten progress events a second: a fast download over many small chunks
/// would otherwise flood the event bus. The final snapshot is never throttled.
const PROGRESS_INTERVAL: std::time::Duration = std::time::Duration::from_millis(100);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    Stable,
    Beta,
    Alpha,
}

impl Channel {
    /// The channel a preference names. `auto` — the default — derives it from
    /// `pre`, the running build's own prerelease tag (empty for a plain
    /// release), so an alpha build polls the alpha manifest out of the box; the
    /// stable endpoint 404s until a stable release exists. An explicit
    /// preference always wins.
    pub fn resolve(pref: &str, pre: &str) -> Self {
        match pref {
            "stable" => Self::Stable,
            "beta" => Self::Beta,
            "alpha" => Self::Alpha,
            _ if pre.contains("alpha") => Self::Alpha,
            _ if pre.contains("beta") => Self::Beta,
            _ => Self::Stable,
        }
    }

    /// Prerelease manifests live on fixed rolling tags in the public releases
    /// repo so the endpoint never moves; stable rides GitHub's `latest` alias.
    /// These three shapes are the contract with tauri-release-kit.
    const fn endpoint(self) -> &'static str {
        match self {
            Self::Stable => {
                "https://github.com/entro314-labs/twister-releases/releases/latest/download/latest.json"
            }
            Self::Beta => {
                "https://github.com/entro314-labs/twister-releases/releases/download/latest-beta/latest.json"
            }
            Self::Alpha => {
                "https://github.com/entro314-labs/twister-releases/releases/download/latest-alpha/latest.json"
            }
        }
    }

    /// What the renderer calls it.
    const fn name(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Beta => "beta",
            Self::Alpha => "alpha",
        }
    }
}

/// What a check found. `None` from [`check_for_update`] means nothing newer.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateMeta {
    pub version: String,
    /// The release notes: the tag's CHANGELOG section, as the pipeline wrote it.
    pub notes: Option<String>,
    pub date: Option<String>,
    /// The channel the manifest came from, so the UI can say which one answered
    /// when the preference is `auto`.
    pub channel: String,
    /// Size in bytes of this platform's bundle, read from the release
    /// pipeline's `size` extension in the update manifest. `None` when the
    /// manifest predates the extension or the lookup fails — size display is
    /// progressive enhancement, never a gate.
    pub download_size: Option<u64>,
}

/// Why self-update is or isn't available for this install, so the UI can point
/// the user at the right update path instead of offering a check that can never
/// work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum InstallSupport {
    /// The in-app updater can replace this install.
    Supported,
    /// Managed by an external package manager (deb/rpm/Flatpak on Linux) — the
    /// Tauri updater can only self-update `AppImage` installs. Only
    /// [`install_support`] on Linux ever builds it; the variant still has to
    /// exist everywhere, because the renderer's union does.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    PackageManager,
}

/// The manifest platform key for the running build (mirrors the updater
/// plugin's own `{{target}}-{{arch}}` resolution for the six targets the
/// release pipeline ships).
const fn manifest_platform_key() -> &'static str {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        "darwin-aarch64"
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        "darwin-x86_64"
    }
    #[cfg(all(target_os = "windows", target_arch = "aarch64"))]
    {
        "windows-aarch64"
    }
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        "windows-x86_64"
    }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        "linux-aarch64"
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        "linux-x86_64"
    }
}

/// A short-lived client for the two metadata fetches below. Neither carries a
/// cookie and neither talks to X: they read a public manifest and a public
/// Atom feed on the releases repository.
fn probe_client() -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .user_agent(concat!("Twister/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|err| crate::error::internal("Building the update client", err))
}

/// Best-effort lookup of this platform's bundle size from the channel
/// manifest's `size` extension (written by tauri-release-kit; absent from
/// manifests published before it existed). Any failure — network, JSON shape,
/// missing key — resolves to `None`.
async fn fetch_download_size(channel: Channel) -> Option<u64> {
    let fetched = tauri::async_runtime::spawn_blocking(move || {
        let body = probe_client()
            .ok()?
            .get(channel.endpoint())
            .send()
            .ok()?
            .error_for_status()
            .ok()?
            .text()
            .ok()?;
        let manifest: serde_json::Value = serde_json::from_str(&body).ok()?;
        manifest
            .get("platforms")?
            .get(manifest_platform_key())?
            .get("size")?
            .as_u64()
    })
    .await;
    fetched.unwrap_or_else(|err| {
        log::debug!("download-size lookup failed: {err}");
        None
    })
}

/// Does the releases host itself answer? `releases.atom` is the cheapest
/// endpoint that exists for every public repository with or without releases:
/// 200 when the repo is live, 404 when it is missing, private or renamed. A
/// transport failure counts as unreachable too — offline and repo-gone are
/// different causes but the same user-visible truth, and neither of them is
/// "no release has been published on this channel yet".
async fn releases_repo_reachable() -> bool {
    let probed = tauri::async_runtime::spawn_blocking(|| {
        probe_client().is_ok_and(|client| {
            client
                .get(format!("{RELEASES_REPO_URL}/releases.atom"))
                .send()
                .is_ok_and(|response| response.status().is_success())
        })
    })
    .await;
    probed.unwrap_or_else(|err| {
        log::debug!("releases-repo probe failed: {err}");
        false
    })
}

/// Poll a channel's manifest, classifying the miss the updater plugin cannot.
/// Shared by [`check_for_update`] and [`stage_update`] so both doors tell the
/// user the same story about the same failure.
async fn check(
    app: &tauri::AppHandle,
    channel: Channel,
) -> Result<Option<tauri_plugin_updater::Update>> {
    let endpoint = channel
        .endpoint()
        .parse()
        .map_err(|err| crate::error::internal("Reading the update endpoint", err))?;
    let updater = app
        .updater_builder()
        .endpoints(vec![endpoint])
        .map_err(AppError::from)?
        .build()
        .map_err(AppError::from)?;

    match updater.check().await {
        Ok(found) => Ok(found),
        // The one miss that is not a failure — until the repository itself
        // turns out to be gone, in which case the pipeline is broken, not
        // pending.
        Err(tauri_plugin_updater::Error::ReleaseNotFound) => {
            if releases_repo_reachable().await {
                log::info!(
                    "no release on the {} channel yet ({RELEASES_REPO_URL} answered)",
                    channel.name()
                );
                Err(AppError::NoRelease(format!(
                    "No release has been published on the {} channel yet.",
                    channel.name()
                )))
            } else {
                log::warn!(
                    "{RELEASES_REPO_URL} did not answer; the update pipeline is unreachable"
                );
                Err(AppError::UpdateSourceUnreachable(format!(
                    "{RELEASES_REPO_URL} did not answer."
                )))
            }
        }
        Err(err) => Err(err.into()),
    }
}

/// The channel this install polls: the preference, with `auto` resolved against
/// the running build's own prerelease tag.
pub fn channel_for(app: &tauri::AppHandle, pref: &str) -> Channel {
    Channel::resolve(pref, app.package_info().version.pre.as_str())
}

/// Report whether the in-app updater can service this install. On Linux the
/// updater only supports `AppImage` (the `APPIMAGE` env var is set by the
/// `AppImage` runtime); deb/rpm/Flatpak installs must update through their
/// package manager. macOS and Windows installs are always self-updatable.
pub fn install_support() -> InstallSupport {
    #[cfg(target_os = "linux")]
    {
        let is_appimage = std::env::var_os("APPIMAGE").is_some();
        let is_flatpak = std::env::var_os("FLATPAK_ID").is_some();
        if is_appimage && !is_flatpak {
            InstallSupport::Supported
        } else {
            InstallSupport::PackageManager
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        InstallSupport::Supported
    }
}

/// Whether an update has been downloaded and staged this session (it installs
/// on the next quit). Lets the UI restore its "restart to update" state after a
/// reload and drive the status bar. A poisoned lock reports `false` — the
/// exit-time installer logs its own state.
pub fn staged(app: &tauri::AppHandle) -> bool {
    app.try_state::<PendingUpdate>()
        .is_some_and(|pending| pending.0.lock().is_ok_and(|guard| guard.is_some()))
}

/// Is a newer build published on this channel?
pub async fn check_for_update(
    app: tauri::AppHandle,
    channel: Channel,
) -> Result<Option<UpdateMeta>> {
    let Some(update) = check(&app, channel).await? else {
        return Ok(None);
    };
    // The size is read BEFORE the user commits to the download.
    let download_size = fetch_download_size(channel).await;
    Ok(Some(UpdateMeta {
        version: update.version.clone(),
        notes: update.body.clone(),
        date: update.date.map(|date| date.to_string()),
        channel: channel.name().into(),
        download_size,
    }))
}

/// Download and verify the channel's latest build and stage it for install on
/// exit. Emits throttled [`EVENT_PROGRESS`] events while it downloads.
pub async fn stage_update(app: tauri::AppHandle, channel: Channel) -> Result<UpdateMeta> {
    let update = check(&app, channel)
        .await?
        .ok_or_else(|| AppError::NotFound("Already on the latest version.".into()))?;

    let meta = UpdateMeta {
        version: update.version.clone(),
        notes: update.body.clone(),
        date: update.date.map(|date| date.to_string()),
        channel: channel.name().into(),
        // The download starts immediately below — the size readout is moot now.
        download_size: None,
    };

    let progress_app = app.clone();
    let mut downloaded: u64 = 0;
    let mut last_emit = std::time::Instant::now()
        .checked_sub(PROGRESS_INTERVAL)
        .unwrap_or_else(std::time::Instant::now);
    let bytes = update
        .download(
            move |chunk, total| {
                downloaded += chunk as u64;
                if last_emit.elapsed() >= PROGRESS_INTERVAL {
                    last_emit = std::time::Instant::now();
                    let _ = progress_app.emit(
                        EVENT_PROGRESS,
                        serde_json::json!({ "downloaded": downloaded, "total": total }),
                    );
                }
            },
            || {},
        )
        .await
        .map_err(|err| crate::error::internal("Downloading the update", err))?;
    // The one emission that is never throttled: the bar has to land on full.
    let _ = app.emit(
        EVENT_PROGRESS,
        serde_json::json!({ "downloaded": bytes.len(), "total": bytes.len() }),
    );

    let pending = app.state::<PendingUpdate>();
    *pending
        .0
        .lock()
        .map_err(|_| AppError::Internal("A lock was poisoned.".into()))? = Some((update, bytes));
    Ok(meta)
}

/// Install the staged update now and relaunch into the new version — the
/// explicit "Restart now" path. Unlike the quit-path installer this surfaces a
/// failure to the caller, and the staged bundle is only consumed on success: a
/// failed install leaves it staged for a retry rather than silently relaunching
/// into the old version.
pub fn restart_and_install(app: &tauri::AppHandle) -> Result<()> {
    let pending = app.state::<PendingUpdate>();
    let mut guard = pending
        .0
        .lock()
        .map_err(|_| AppError::Internal("A lock was poisoned.".into()))?;
    let (update, bytes) = guard
        .as_ref()
        .ok_or_else(|| AppError::NotFound("No update is staged.".into()))?;
    update
        .install(bytes)
        .map_err(|err| crate::error::internal("Installing the update", err))?;
    *guard = None;
    drop(guard);
    log::info!("staged update installed; restarting into the new version");
    app.restart();
}

/// Quit path: install whatever is staged, quietly. Called from
/// `RunEvent::ExitRequested`, so the on-disk bundle is replaced while no code is
/// running out of it. Best-effort — the app is exiting either way, and a failed
/// install must not wedge shutdown.
pub fn install_pending_on_exit(app: &tauri::AppHandle) {
    let Some(pending) = app.try_state::<PendingUpdate>() else {
        return;
    };
    let staged = match pending.0.lock() {
        Ok(mut guard) => guard.take(),
        Err(err) => {
            log::warn!("skipping the deferred install — the pending update is poisoned: {err}");
            return;
        }
    };
    let Some((update, bytes)) = staged else {
        return;
    };
    if let Err(err) = update.install(&bytes) {
        log::error!("the staged update failed to install on exit: {err}");
    } else {
        log::info!("staged update installed; the next launch runs the new version");
    }
}

#[cfg(test)]
mod tests {
    use super::{Channel, RELEASES_REPO_URL};

    /// `auto` follows the running build's own tag, so an alpha build polls the
    /// alpha manifest rather than a stable endpoint that 404s until the first
    /// stable release exists. An explicit preference always wins.
    #[test]
    fn auto_follows_the_running_build() {
        assert_eq!(Channel::resolve("auto", "alpha.1"), Channel::Alpha);
        assert_eq!(Channel::resolve("auto", "beta.0"), Channel::Beta);
        assert_eq!(Channel::resolve("auto", ""), Channel::Stable);
        assert_eq!(Channel::resolve("stable", "alpha.1"), Channel::Stable);
        assert_eq!(Channel::resolve("beta", ""), Channel::Beta);
        assert_eq!(Channel::resolve("alpha", ""), Channel::Alpha);
    }

    /// Every channel manifest is an asset of the releases repo. If an endpoint
    /// is repointed without moving [`RELEASES_REPO_URL`] with it, the
    /// unreachable probe interrogates the wrong host and a broken pipeline is
    /// mislabelled as the calm pre-first-release state again.
    #[test]
    fn endpoints_live_on_the_releases_repo() {
        for channel in [Channel::Stable, Channel::Beta, Channel::Alpha] {
            assert!(
                channel.endpoint().starts_with(RELEASES_REPO_URL),
                "{channel:?} endpoint is not hosted on {RELEASES_REPO_URL}"
            );
        }
    }

    /// tauri-release-kit publishes prerelease manifests on rolling tags
    /// (`latest-alpha` / `latest-beta`) and stable through GitHub's `latest`
    /// alias; these three shapes are the contract between the two repos.
    #[test]
    fn endpoints_match_the_release_kit_convention() {
        assert!(
            Channel::Stable
                .endpoint()
                .ends_with("/releases/latest/download/latest.json")
        );
        assert!(
            Channel::Beta
                .endpoint()
                .ends_with("/releases/download/latest-beta/latest.json")
        );
        assert!(
            Channel::Alpha
                .endpoint()
                .ends_with("/releases/download/latest-alpha/latest.json")
        );
    }
}
