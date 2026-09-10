//! Export writers: the store as CSV, JSON or Markdown, and the save dialog.
//!
//! CSV is written for a spreadsheet to open, which means a byte-order mark
//! so Excel reads the UTF-8, and a leading apostrophe on any cell that
//! begins with a formula character — a bio reading `=HYPERLINK(...)` must
//! stay text.

use std::fmt::Write as _;

use tauri_plugin_dialog::DialogExt;

use crate::db::{Post, User};
use crate::error::{AppError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Csv,
    Json,
    Markdown,
}

impl Format {
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "csv" => Ok(Self::Csv),
            "json" => Ok(Self::Json),
            "markdown" | "md" => Ok(Self::Markdown),
            other => Err(AppError::InvalidInput(format!("Unknown format `{other}`."))),
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Csv => "csv",
            Self::Json => "json",
            Self::Markdown => "md",
        }
    }
}

fn csv_cell(value: &str) -> String {
    let mut text = value.replace('\r', "");
    if text.starts_with(['=', '+', '-', '@', '\t']) {
        text.insert(0, '\'');
    }
    if text.contains([',', '"', '\n']) {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text
    }
}

fn csv_line(cells: &[String]) -> String {
    let mut line = cells
        .iter()
        .map(|c| csv_cell(c))
        .collect::<Vec<_>>()
        .join(",");
    line.push_str("\r\n");
    line
}

fn flag(value: Option<bool>) -> String {
    match value {
        Some(true) => "yes".into(),
        Some(false) => "no".into(),
        None => String::new(),
    }
}

pub fn users_csv(users: &[User]) -> String {
    let mut out = String::from('\u{feff}');
    out.push_str(&csv_line(&[
        "handle".into(),
        "name".into(),
        "bio".into(),
        "location".into(),
        "website".into(),
        "followers".into(),
        "following".into(),
        "posts".into(),
        "verified".into(),
        "protected".into(),
        "follows_me".into(),
        "followed_by_me".into(),
        "created_at".into(),
        "source".into(),
        "last_seen".into(),
        "url".into(),
    ]));
    for user in users {
        out.push_str(&csv_line(&[
            user.handle.clone(),
            user.name.clone(),
            user.bio.clone(),
            user.location.clone(),
            user.website.clone(),
            user.followers.to_string(),
            user.following.to_string(),
            user.posts.to_string(),
            flag(Some(user.verified)),
            flag(Some(user.protected)),
            flag(user.follows_me),
            flag(user.followed_by_me),
            user.created_at.clone(),
            user.source.clone(),
            user.last_seen.clone(),
            format!("https://x.com/{}", user.handle),
        ]));
    }
    out
}

pub fn posts_csv(posts: &[Post]) -> String {
    let mut out = String::from('\u{feff}');
    out.push_str(&csv_line(&[
        "id".into(),
        "author".into(),
        "created_at".into(),
        "kind".into(),
        "text".into(),
        "likes".into(),
        "reposts".into(),
        "replies".into(),
        "views".into(),
        "bookmarked".into(),
        "media".into(),
        "source".into(),
        "url".into(),
    ]));
    for post in posts {
        out.push_str(&csv_line(&[
            post.id.clone(),
            post.author_handle.clone(),
            post.created_at.clone(),
            post.kind.clone(),
            post.text.clone(),
            post.likes.to_string(),
            post.reposts.to_string(),
            post.replies.to_string(),
            post.views.to_string(),
            flag(Some(post.bookmarked)),
            post.media
                .iter()
                .map(|m| m.url.clone())
                .collect::<Vec<_>>()
                .join(" "),
            post.source.clone(),
            post.url(),
        ]));
    }
    out
}

pub fn posts_markdown(posts: &[Post]) -> String {
    let mut out = String::new();
    for post in posts {
        let when = if post.created_at.is_empty() {
            String::new()
        } else {
            format!(" · {}", post.created_at)
        };
        let _ = write!(out, "### @{}{when}\n\n", post.author_handle);
        for line in post.text.lines() {
            out.push_str("> ");
            out.push_str(line);
            out.push('\n');
        }
        out.push('\n');
        for media in &post.media {
            let _ = writeln!(out, "- {}: {}", media.kind, media.url);
        }
        if !post.media.is_empty() {
            out.push('\n');
        }
        let _ = write!(
            out,
            "[{}]({}) · {} likes · {} reposts\n\n---\n\n",
            post.url(),
            post.url(),
            post.likes,
            post.reposts
        );
    }
    out
}

pub fn users_markdown(users: &[User]) -> String {
    let mut out =
        String::from("| Handle | Name | Followers | Following | Bio |\n|---|---|---:|---:|---|\n");
    for user in users {
        let _ = writeln!(
            out,
            "| [@{0}](https://x.com/{0}) | {1} | {2} | {3} | {4} |",
            user.handle,
            user.name.replace('|', "\\|"),
            user.followers,
            user.following,
            user.bio.replace('|', "\\|").replace('\n', " ")
        );
    }
    out
}

pub fn render_users(users: &[User], format: Format) -> Result<String> {
    Ok(match format {
        Format::Csv => users_csv(users),
        Format::Json => serde_json::to_string_pretty(users)?,
        Format::Markdown => users_markdown(users),
    })
}

pub fn render_posts(posts: &[Post], format: Format) -> Result<String> {
    Ok(match format {
        Format::Csv => posts_csv(posts),
        Format::Json => serde_json::to_string_pretty(posts)?,
        Format::Markdown => posts_markdown(posts),
    })
}

/// Asks where to save, writes, and returns the path — or `None` when the
/// dialog was dismissed, which is not an error.
pub fn save(app: &tauri::AppHandle, suggested: &str, contents: &str) -> Result<Option<String>> {
    let chosen = app
        .dialog()
        .file()
        .set_file_name(suggested)
        .blocking_save_file();
    let Some(path) = chosen else {
        return Ok(None);
    };
    let path = path
        .into_path()
        .map_err(|e| AppError::Internal(format!("Unusable save location: {e}")))?;
    std::fs::write(&path, contents)?;
    Ok(Some(path.display().to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Media;

    #[test]
    fn csv_cells_are_quoted_and_formula_neutral() {
        assert_eq!(csv_cell("plain"), "plain");
        assert_eq!(csv_cell("a,b"), "\"a,b\"");
        assert_eq!(csv_cell("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(csv_cell("=HYPERLINK(\"x\")"), "\"'=HYPERLINK(\"\"x\"\")\"");
        assert_eq!(csv_cell("@handle"), "'@handle");
        assert_eq!(csv_cell("-1"), "'-1");
        assert_eq!(csv_cell("line\r\nbreak"), "\"line\nbreak\"");
    }

    #[test]
    fn users_csv_starts_with_a_bom_and_a_header() {
        let csv = users_csv(&[User {
            handle: "alice".into(),
            name: "Alice, PhD".into(),
            follows_me: Some(true),
            ..User::default()
        }]);
        assert!(csv.starts_with('\u{feff}'));
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].starts_with("\u{feff}handle,name,bio"));
        assert!(lines[1].starts_with("alice,\"Alice, PhD\","));
        assert!(lines[1].contains(",yes,,"));
        assert!(lines[1].ends_with("https://x.com/alice"));
    }

    #[test]
    fn posts_render_in_every_format() {
        let posts = vec![Post {
            id: "1".into(),
            author_handle: "alice".into(),
            text: "hello\nworld".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
            kind: "post".into(),
            media: vec![Media {
                kind: "photo".into(),
                url: "https://pbs.twimg.com/media/a.jpg".into(),
            }],
            ..Post::default()
        }];
        let csv = render_posts(&posts, Format::Csv).expect("csv");
        assert!(csv.contains("\"hello\nworld\""));
        let json = render_posts(&posts, Format::Json).expect("json");
        assert!(json.contains("\"authorHandle\": \"alice\""));
        let md = render_posts(&posts, Format::Markdown).expect("md");
        assert!(md.contains("### @alice · 2026-01-01T00:00:00Z"));
        assert!(md.contains("> hello\n> world\n"));
        assert!(md.contains("- photo: https://pbs.twimg.com/media/a.jpg"));
        let table = render_users(&[User::default()], Format::Markdown).expect("md");
        assert!(table.starts_with("| Handle |"));
        assert_eq!(Format::parse("md").expect("md"), Format::Markdown);
        assert!(Format::parse("xlsx").is_err());
    }
}
