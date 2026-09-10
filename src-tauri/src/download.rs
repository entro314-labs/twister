//! Media downloads: a post's photos and videos, saved to a folder in
//! Downloads.
//!
//! The URLs come from the capture hook — X's own API response listed the
//! original photo and every mp4 variant, and the store kept the best of each
//! — or, for a post the hook never saw, from the images the page drew. The
//! fetch itself is plain HTTPS from Rust to X's public CDN, with no cookies
//! and nothing that identifies the account.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;
use tauri::AppHandle;

use crate::capture::valid_id;
use crate::db::{Db, Media};
use crate::error::{AppError, Result};
use crate::site::{self, valid_handle};

const FOLDER: &str = "Twister";
const MAX_ITEMS: usize = 8;
/// One post's worth of media, however long a video is.
const MAX_BYTES: u64 = 512 * 1024 * 1024;

/// What the page sends when its download button is pressed.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Request {
    pub post_id: String,
    pub handle: String,
    /// Photo URLs read from the page, for a post the store has no media for.
    pub images: Vec<String>,
}

pub fn dir() -> Result<PathBuf> {
    let base = dirs::download_dir()
        .or_else(dirs::home_dir)
        .ok_or_else(|| AppError::Internal("No Downloads folder.".into()))?;
    let dir = base.join(FOLDER);
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn twimg(url: &str) -> Option<url::Url> {
    let parsed = url::Url::parse(url).ok()?;
    let host = parsed.host_str()?;
    (parsed.scheme() == "https" && (host == "twimg.com" || host.ends_with(".twimg.com")))
        .then_some(parsed)
}

/// `pbs.twimg.com/media/x?format=jpg&name=small` → the original pixels.
pub fn original_image(url: &str) -> Option<String> {
    let mut parsed = twimg(url)?;
    if !parsed.path().starts_with("/media/") {
        return None;
    }
    let pairs: Vec<(String, String)> = parsed
        .query_pairs()
        .filter(|(k, _)| k != "name")
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    parsed
        .query_pairs_mut()
        .clear()
        .extend_pairs(pairs)
        .append_pair("name", "orig");
    Some(parsed.to_string())
}

pub fn extension(url: &str) -> String {
    let parsed = twimg(url);
    let from_query = parsed.as_ref().and_then(|p| {
        p.query_pairs()
            .find(|(k, _)| k == "format")
            .map(|(_, v)| v.into_owned())
    });
    let from_path = parsed.as_ref().and_then(|p| {
        Path::new(p.path())
            .extension()
            .map(|e| e.to_string_lossy().into_owned())
    });
    let ext = from_query
        .or(from_path)
        .unwrap_or_else(|| "bin".into())
        .to_lowercase();
    if ext == "jpeg" { "jpg".into() } else { ext }
}

/// Snowflake ids carry their creation time.
pub fn date_from_id(id: &str) -> Option<String> {
    let id: u64 = id.parse().ok()?;
    let ms = (id >> 22) + 1_288_834_974_657;
    let when = chrono::DateTime::from_timestamp_millis(i64::try_from(ms).ok()?)?;
    Some(when.format("%Y%m%d").to_string())
}

/// `handle_20260101_1234567890_1.jpg`
pub fn file_name(handle: &str, id: &str, index: usize, count: usize, url: &str) -> String {
    let date = date_from_id(id).unwrap_or_else(|| "undated".into());
    let suffix = if count > 1 {
        format!("_{}", index + 1)
    } else {
        String::new()
    };
    format!("{handle}_{date}_{id}{suffix}.{}", extension(url))
}

/// Which files to fetch for a request: the store's media when it has any,
/// else the page's images at original size.
pub fn plan(request: &Request, stored: Option<&[Media]>) -> Result<Vec<(String, String)>> {
    if !valid_id(&request.post_id) || !valid_handle(&request.handle) {
        return Err(AppError::InvalidInput("That is not a post.".into()));
    }
    let urls: Vec<String> = match stored {
        Some(media) if !media.is_empty() => media.iter().map(|m| m.url.clone()).collect(),
        _ => request
            .images
            .iter()
            .filter_map(|u| original_image(u))
            .collect(),
    };
    let urls: Vec<String> = urls
        .into_iter()
        .filter(|u| twimg(u).is_some())
        .take(MAX_ITEMS)
        .collect();
    if urls.is_empty() {
        return Err(AppError::NotFound(
            "Twister has not seen any media for this post yet. Scroll it into view once and try again.".into(),
        ));
    }
    let count = urls.len();
    Ok(urls
        .into_iter()
        .enumerate()
        .map(|(i, url)| {
            let name = file_name(&request.handle, &request.post_id, i, count, &url);
            (url, name)
        })
        .collect())
}

/// Fetches in the background and says how it went in the status bar.
pub fn start(app: &AppHandle, request: Request) -> Result<()> {
    let stored = app.state::<Db>().post(&request.post_id)?.map(|p| p.media);
    let files = plan(&request, stored.as_deref())?;
    let dir = dir()?;
    let app = app.clone();
    site::notify(
        &app,
        format!(
            "Downloading {} file{}…",
            files.len(),
            if files.len() == 1 { "" } else { "s" }
        ),
    );
    std::thread::spawn(move || {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(300))
            .user_agent("Twister")
            .build();
        let Ok(client) = client else {
            site::notify(&app, "Could not start the download.");
            return;
        };
        let mut saved = 0;
        for (url, name) in &files {
            match fetch(&client, url, &dir.join(name)) {
                Ok(()) => saved += 1,
                Err(err) => log::warn!("download of {url} failed: {err}"),
            }
        }
        site::notify(
            &app,
            if saved == files.len() {
                format!("Saved {saved} to Downloads/{FOLDER}")
            } else {
                format!("Saved {saved} of {} to Downloads/{FOLDER}", files.len())
            },
        );
    });
    Ok(())
}

fn fetch(client: &reqwest::blocking::Client, url: &str, path: &Path) -> Result<()> {
    let response = client
        .get(url)
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .map_err(|e| AppError::Internal(format!("Fetch failed: {e}")))?;
    if response.content_length().is_some_and(|len| len > MAX_BYTES) {
        return Err(AppError::Internal("File too large.".into()));
    }
    let bytes = response
        .bytes()
        .map_err(|e| AppError::Internal(format!("Read failed: {e}")))?;
    let tmp = path.with_extension("part");
    std::fs::write(&tmp, &bytes)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

use tauri::Manager;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_urls_are_upgraded_to_the_original() {
        assert_eq!(
            original_image("https://pbs.twimg.com/media/abc?format=jpg&name=small").expect("ok"),
            "https://pbs.twimg.com/media/abc?format=jpg&name=orig"
        );
        assert_eq!(
            original_image("https://pbs.twimg.com/media/abc.jpg").expect("ok"),
            "https://pbs.twimg.com/media/abc.jpg?name=orig"
        );
        assert!(original_image("https://pbs.twimg.com/profile_images/a.jpg").is_none());
        assert!(original_image("https://example.com/media/a.jpg").is_none());
    }

    #[test]
    fn extensions_and_dates_come_from_the_url_and_the_id() {
        assert_eq!(
            extension("https://pbs.twimg.com/media/abc?format=jpg&name=orig"),
            "jpg"
        );
        assert_eq!(extension("https://pbs.twimg.com/media/abc.png"), "png");
        assert_eq!(
            extension("https://video.twimg.com/a/b/720x1280/x.mp4?tag=12"),
            "mp4"
        );
        assert_eq!(extension("https://pbs.twimg.com/media/abc"), "bin");
        // 1288834974657 + (id >> 22) ms.
        assert_eq!(date_from_id("1600000000000000000").expect("ok"), "20221206");
        assert!(date_from_id("nope").is_none());
        assert_eq!(
            file_name(
                "alice",
                "1600000000000000000",
                1,
                3,
                "https://pbs.twimg.com/media/a?format=png"
            ),
            "alice_20221206_1600000000000000000_2.png"
        );
    }

    #[test]
    fn plans_prefer_the_store_and_fall_back_to_page_images() {
        let request = Request {
            post_id: "1".into(),
            handle: "alice".into(),
            images: vec![
                "https://pbs.twimg.com/media/a?format=jpg&name=small".into(),
                "https://evil.example/a.jpg".into(),
            ],
        };
        let stored = vec![Media {
            kind: "video".into(),
            url: "https://video.twimg.com/v/720x1280/a.mp4".into(),
        }];
        let planned = plan(&request, Some(&stored)).expect("ok");
        assert_eq!(planned.len(), 1);
        assert!(planned[0].0.ends_with("a.mp4"));
        let fallback = plan(&request, None).expect("ok");
        assert_eq!(fallback.len(), 1);
        assert!(fallback[0].0.ends_with("name=orig"));
        let empty = Request {
            images: vec![],
            ..request.clone()
        };
        assert!(plan(&empty, Some(&[])).is_err());
        let bad = Request {
            post_id: "x".into(),
            ..request
        };
        assert!(plan(&bad, None).is_err());
    }
}
