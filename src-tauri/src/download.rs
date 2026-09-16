//! Media downloads: a post's photos and videos, saved to a folder in
//! Downloads.
//!
//! The URLs come from the capture hook — the site's own API response listed
//! the original photo and every video variant, and the store kept the best
//! of each — or, on X, for a post the hook never saw, from the images the
//! page drew. The fetch itself is plain HTTPS from Rust to the network's
//! public CDN, with no cookies and nothing that identifies the account.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;
use tauri::AppHandle;

use crate::db::{Db, Post};
use crate::error::{AppError, Result};
use crate::network::Network;
use crate::site;

const FOLDER: &str = "Twister";
const MAX_ITEMS: usize = 8;
/// One post's worth of media, however long a video is.
const MAX_BYTES: u64 = 512 * 1024 * 1024;

/// What the page sends when its download button is pressed. The network is
/// the calling tab's.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Request {
    pub post_id: String,
    pub handle: String,
    /// Photo URLs read from the page, for a post the store has no media for.
    /// X only: its page draws the same CDN URL the API names.
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

fn on_cdn(network: Network, url: &str) -> Option<url::Url> {
    let parsed = url::Url::parse(url).ok()?;
    let host = parsed.host_str()?;
    (parsed.scheme() == "https" && network.media_host_ok(host)).then_some(parsed)
}

/// `pbs.twimg.com/media/x?format=jpg&name=small` → the original pixels.
pub fn original_image(url: &str) -> Option<String> {
    let mut parsed = on_cdn(Network::X, url)?;
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
    let parsed = url::Url::parse(url).ok();
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
    // A CDN path with no extension, or one that is not a file's: `m3u8` is a
    // playlist, and Bluesky's image paths end in a content hash.
    let ext = if ext.len() > 5 || ext.is_empty() {
        "bin".to_string()
    } else {
        ext
    };
    if ext == "jpeg" { "jpg".into() } else { ext }
}

/// Snowflake ids carry their creation time.
pub fn date_from_id(id: &str) -> Option<String> {
    let id: u64 = id.parse().ok()?;
    let ms = (id >> 22) + 1_288_834_974_657;
    let when = chrono::DateTime::from_timestamp_millis(i64::try_from(ms).ok()?)?;
    Some(when.format("%Y%m%d").to_string())
}

/// The day a post was made, from its snowflake on X and from what the
/// store recorded elsewhere.
fn date_of(network: Network, id: &str, created_at: &str) -> String {
    let from_store = || {
        chrono::DateTime::parse_from_rfc3339(created_at)
            .ok()
            .map(|when| when.format("%Y%m%d").to_string())
    };
    match network {
        Network::X => date_from_id(id),
        _ => from_store(),
    }
    .unwrap_or_else(|| "undated".into())
}

/// `handle_20260101_1234567890_1.jpg`. Off X the id in the name is the
/// short one a URL carries — the rkey, the shortcode — since a DID-bearing
/// at-URI is not a file name; the shortcode and the date come from the
/// stored post when there is one.
pub fn file_name(
    network: Network,
    handle: &str,
    id: &str,
    stored: Option<&Post>,
    index: usize,
    count: usize,
    url: &str,
) -> String {
    let (slug, created_at) = stored.map_or(("", ""), |p| (p.slug.as_str(), p.created_at.as_str()));
    let date = date_of(network, id, created_at);
    let short = match network {
        Network::X => id,
        Network::Bluesky => id.rsplit('/').next().unwrap_or(id),
        Network::Threads | Network::Instagram => {
            if slug.is_empty() {
                id
            } else {
                slug
            }
        }
    };
    let suffix = if count > 1 {
        format!("_{}", index + 1)
    } else {
        String::new()
    };
    format!("{handle}_{date}_{short}{suffix}.{}", extension(url))
}

/// Which files to fetch for a request: the store's media when it has any,
/// else — on X — the page's images at original size.
pub fn plan(
    network: Network,
    request: &Request,
    stored: Option<&Post>,
) -> Result<Vec<(String, String)>> {
    if !network.valid_id(&request.post_id) || !network.valid_handle(&request.handle) {
        return Err(AppError::InvalidInput("That is not a post.".into()));
    }
    let urls: Vec<String> = match stored {
        Some(post) if !post.media.is_empty() => post.media.iter().map(|m| m.url.clone()).collect(),
        _ if network == Network::X => request
            .images
            .iter()
            .filter_map(|u| original_image(u))
            .collect(),
        _ => Vec::new(),
    };
    let urls: Vec<String> = urls
        .into_iter()
        .filter(|u| on_cdn(network, u).is_some())
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
            let name = file_name(
                network,
                &request.handle,
                &request.post_id,
                stored,
                i,
                count,
                &url,
            );
            (url, name)
        })
        .collect())
}

/// Fetches in the background and says how it went in the status bar.
pub fn start(app: &AppHandle, network: Network, request: Request) -> Result<()> {
    let stored = app.state::<Db>().post(network, &request.post_id)?;
    let files = plan(network, &request, stored.as_ref())?;
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
    use crate::db::Media;

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
        assert_eq!(
            extension("https://cdn.bsky.app/img/feed_fullsize/plain/did:plc:a/bafkreic6nb7dy"),
            "bin"
        );
        // 1288834974657 + (id >> 22) ms.
        assert_eq!(date_from_id("1600000000000000000").expect("ok"), "20221206");
        assert!(date_from_id("nope").is_none());
        assert_eq!(
            file_name(
                Network::X,
                "alice",
                "1600000000000000000",
                None,
                1,
                3,
                "https://pbs.twimg.com/media/a?format=png"
            ),
            "alice_20221206_1600000000000000000_2.png"
        );
        assert_eq!(
            file_name(
                Network::Bluesky,
                "bsky.app",
                "at://did:plc:a/app.bsky.feed.post/3l6oveex3ii2l",
                Some(&Post {
                    created_at: "2024-10-17T07:06:51Z".into(),
                    ..Post::default()
                }),
                0,
                1,
                "https://cdn.bsky.app/img/feed_fullsize/plain/did:plc:a/bafy"
            ),
            "bsky.app_20241017_3l6oveex3ii2l.bin"
        );
        assert_eq!(
            file_name(
                Network::Threads,
                "zuck",
                "398",
                Some(&Post {
                    slug: "DdU1-6okapE".into(),
                    ..Post::default()
                }),
                0,
                1,
                "https://scontent.cdninstagram.com/v/a.jpg?x=1"
            ),
            "zuck_undated_DdU1-6okapE.jpg"
        );
    }

    #[test]
    fn plans_prefer_the_store_and_fall_back_to_page_images_on_x() {
        let request = Request {
            post_id: "1".into(),
            handle: "alice".into(),
            images: vec![
                "https://pbs.twimg.com/media/a?format=jpg&name=small".into(),
                "https://evil.example/a.jpg".into(),
            ],
        };
        let stored = Post {
            network: Network::X,
            id: "1".into(),
            media: vec![Media {
                kind: "video".into(),
                url: "https://video.twimg.com/v/720x1280/a.mp4".into(),
            }],
            ..Post::default()
        };
        let planned = plan(Network::X, &request, Some(&stored)).expect("ok");
        assert_eq!(planned.len(), 1);
        assert!(planned[0].0.ends_with("a.mp4"));
        let fallback = plan(Network::X, &request, None).expect("ok");
        assert_eq!(fallback.len(), 1);
        assert!(fallback[0].0.ends_with("name=orig"));
        let empty = Request {
            images: vec![],
            ..request.clone()
        };
        let no_media = Post {
            media: vec![],
            ..stored
        };
        assert!(plan(Network::X, &empty, Some(&no_media)).is_err());
        let bad = Request {
            post_id: "x".into(),
            ..request.clone()
        };
        assert!(plan(Network::X, &bad, None).is_err());
        // Off X the page's images are not trusted, and the store's URLs
        // must be that network's CDN.
        let bluesky = Request {
            post_id: "at://did:plc:a/app.bsky.feed.post/3k".into(),
            handle: "a.bsky.social".into(),
            images: vec!["https://cdn.bsky.app/img/a".into()],
        };
        assert!(plan(Network::Bluesky, &bluesky, None).is_err());
        let stored = Post {
            network: Network::Bluesky,
            media: vec![
                Media {
                    kind: "photo".into(),
                    url: "https://cdn.bsky.app/img/feed_fullsize/plain/did:plc:a/bafy".into(),
                },
                Media {
                    kind: "photo".into(),
                    url: "https://pbs.twimg.com/media/a.jpg".into(),
                },
            ],
            ..Post::default()
        };
        assert_eq!(
            plan(Network::Bluesky, &bluesky, Some(&stored))
                .expect("ok")
                .len(),
            1
        );
    }
}
