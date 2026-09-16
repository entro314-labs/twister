//! What the page sends the store, and the checks it goes through first.
//!
//! Each network's `capture.js` reads the site's own API responses as the
//! page receives them and batches the people and posts it finds into
//! `site_capture`. The page is not trusted: anything running on the site
//! could call that command, so every field is bounded here before it reaches
//! SQLite — ids and handles must fit the network's own rule, media URLs must
//! be its CDN, and text is cut to a size that cannot bloat the file. The
//! network is the calling tab's, never the page's word.

use serde::Deserialize;

use crate::db::{Media, Post, User};
use crate::error::{AppError, Result};
use crate::network::Network;

/// Per call. The hook batches a few hundred milliseconds of traffic, which is
/// never more than a couple of timeline pages.
const MAX_ITEMS: usize = 1000;
const MAX_SHORT: usize = 200;
const MAX_BIO: usize = 2000;
const MAX_TEXT: usize = 20_000;
const MAX_MEDIA: usize = 8;
const MAX_URL: usize = 1000;

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Batch {
    /// The operation the page called, from the request: X's GraphQL name,
    /// Bluesky's XRPC method, Meta's query name.
    pub source: String,
    pub users: Vec<User>,
    pub posts: Vec<Post>,
}

/// A source is an operation name: `UserTweets`, `app.bsky.feed.getAuthorFeed`,
/// `BarcelonaProfileThreadsTabQuery`.
pub fn valid_source(source: &str) -> bool {
    !source.is_empty()
        && source.len() <= 80
        && source
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.')
}

fn media_url_ok(network: Network, url: &str) -> bool {
    url.len() <= MAX_URL
        && url::Url::parse(url).is_ok_and(|parsed| {
            parsed.scheme() == "https"
                && parsed
                    .host_str()
                    .is_some_and(|host| network.media_host_ok(host))
        })
}

fn clip(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    // Cut on a character boundary, never inside one.
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_string()
}

fn rfc3339_or_empty(value: &str) -> String {
    if value.len() <= 40 && chrono::DateTime::parse_from_rfc3339(value).is_ok() {
        value.to_string()
    } else {
        String::new()
    }
}

/// Everything that passes, with the rest dropped and counted. A batch with
/// more items than the cap is refused whole: that is not traffic, that is
/// something else calling the command.
pub fn sanitize(network: Network, batch: Batch) -> Result<(Vec<User>, Vec<Post>, usize)> {
    if batch.users.len() > MAX_ITEMS || batch.posts.len() > MAX_ITEMS {
        return Err(AppError::InvalidInput("Capture batch too large.".into()));
    }
    let source = if valid_source(&batch.source) {
        batch.source
    } else {
        "Unknown".to_string()
    };
    let mut dropped = 0;
    let mut users = Vec::with_capacity(batch.users.len());
    for user in batch.users {
        if !network.valid_id(&user.id) || !network.valid_handle(&user.handle) {
            dropped += 1;
            continue;
        }
        users.push(User {
            network,
            id: user.id,
            handle: user.handle,
            name: clip(&user.name, MAX_SHORT),
            bio: clip(&user.bio, MAX_BIO),
            location: clip(&user.location, MAX_SHORT),
            website: clip(&user.website, MAX_URL),
            followers: user.followers.max(0),
            following: user.following.max(0),
            posts: user.posts.max(0),
            verified: user.verified,
            protected: user.protected,
            avatar: if media_url_ok(network, &user.avatar) {
                user.avatar
            } else {
                String::new()
            },
            created_at: rfc3339_or_empty(&user.created_at),
            follows_me: user.follows_me,
            followed_by_me: user.followed_by_me,
            source: source.clone(),
            first_seen: String::new(),
            last_seen: String::new(),
        });
    }
    let mut posts = Vec::with_capacity(batch.posts.len());
    for post in batch.posts {
        if !network.valid_id(&post.id)
            || !network.valid_handle(&post.author_handle)
            || !network.valid_slug(&post.slug)
        {
            dropped += 1;
            continue;
        }
        let kind = match post.kind.as_str() {
            "reply" | "repost" | "quote" => post.kind,
            _ => "post".to_string(),
        };
        let media: Vec<Media> = post
            .media
            .into_iter()
            .filter(|m| {
                matches!(m.kind.as_str(), "photo" | "video" | "animated_gif")
                    && media_url_ok(network, &m.url)
            })
            .take(MAX_MEDIA)
            .collect();
        let optional_id = |id: String| {
            if network.valid_id(&id) {
                id
            } else {
                String::new()
            }
        };
        posts.push(Post {
            network,
            id: post.id,
            slug: post.slug,
            author_id: optional_id(post.author_id),
            author_handle: post.author_handle,
            text: clip(&post.text, MAX_TEXT),
            created_at: rfc3339_or_empty(&post.created_at),
            kind,
            lang: clip(&post.lang, 16),
            likes: post.likes.max(0),
            reposts: post.reposts.max(0),
            replies: post.replies.max(0),
            views: post.views.max(0),
            bookmarked: post.bookmarked,
            media,
            repost_of: optional_id(post.repost_of),
            reply_to: optional_id(post.reply_to),
            quoted_id: optional_id(post.quoted_id),
            source: source.clone(),
            first_seen: String::new(),
            last_seen: String::new(),
        });
    }
    Ok((users, posts, dropped))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sources_and_media_hosts_are_checked() {
        assert!(valid_source("UserTweets"));
        assert!(valid_source("app.bsky.feed.getAuthorFeed"));
        assert!(!valid_source("User Tweets"));
        assert!(!valid_source(""));
        let x = Network::X;
        assert!(media_url_ok(
            x,
            "https://pbs.twimg.com/media/a.jpg?name=orig"
        ));
        assert!(media_url_ok(
            x,
            "https://video.twimg.com/ext_tw_video/1/pu/vid/720x1280/a.mp4"
        ));
        assert!(!media_url_ok(x, "http://pbs.twimg.com/media/a.jpg"));
        assert!(!media_url_ok(x, "https://evil.example/twimg.com/a.jpg"));
        assert!(!media_url_ok(x, "https://twimg.com.evil.example/a.jpg"));
        assert!(!media_url_ok(x, "https://cdn.bsky.app/img/a"));
        assert!(media_url_ok(
            Network::Bluesky,
            "https://cdn.bsky.app/img/feed_fullsize/plain/did:plc:a/bafy"
        ));
    }

    #[test]
    fn rows_are_checked_against_the_calling_networks_rules() {
        let batch = Batch {
            source: "app.bsky.feed.getAuthorFeed".into(),
            users: vec![User {
                id: "did:plc:z72i7hdynmk6r22z27h6tvur".into(),
                handle: "bsky.app".into(),
                ..User::default()
            }],
            posts: vec![Post {
                id: "at://did:plc:z72i7hdynmk6r22z27h6tvur/app.bsky.feed.post/3l6oveex3ii2l".into(),
                author_handle: "bsky.app".into(),
                ..Post::default()
            }],
        };
        // On X these are nonsense; on Bluesky they are the rule.
        let (users, posts, dropped) = sanitize(Network::X, batch.clone()).expect("ok");
        assert!(users.is_empty() && posts.is_empty());
        assert_eq!(dropped, 2);
        let (users, posts, dropped) = sanitize(Network::Bluesky, batch).expect("ok");
        assert_eq!(users.len(), 1);
        assert_eq!(users[0].network, Network::Bluesky);
        assert_eq!(posts.len(), 1);
        assert_eq!(dropped, 0);
        // Meta rows carry a shortcode; X rows must not.
        let (_, posts, _) = sanitize(
            Network::Threads,
            Batch {
                source: "Q".into(),
                users: vec![],
                posts: vec![Post {
                    id: "398".into(),
                    slug: "DdU1-6okapE".into(),
                    author_handle: "zuck".into(),
                    ..Post::default()
                }],
            },
        )
        .expect("ok");
        assert_eq!(
            posts[0].url(),
            "https://www.threads.com/@zuck/post/DdU1-6okapE"
        );
        let (_, posts, dropped) = sanitize(
            Network::X,
            Batch {
                source: "Q".into(),
                users: vec![],
                posts: vec![Post {
                    id: "398".into(),
                    slug: "abc".into(),
                    author_handle: "dom".into(),
                    ..Post::default()
                }],
            },
        )
        .expect("ok");
        assert!(posts.is_empty());
        assert_eq!(dropped, 1);
    }

    #[test]
    fn sanitize_drops_bad_rows_and_clips_text() {
        let batch = Batch {
            source: "Following".into(),
            users: vec![
                User {
                    id: "1".into(),
                    handle: "alice".into(),
                    bio: "x".repeat(5000),
                    followers: -3,
                    avatar: "https://evil.example/a.png".into(),
                    created_at: "not a date".into(),
                    ..User::default()
                },
                User {
                    id: "nope".into(),
                    handle: "bob".into(),
                    ..User::default()
                },
                User {
                    id: "3".into(),
                    handle: "has space".into(),
                    ..User::default()
                },
            ],
            posts: vec![
                Post {
                    id: "10".into(),
                    author_handle: "alice".into(),
                    kind: "weird".into(),
                    reply_to: "abc".into(),
                    media: vec![
                        Media {
                            kind: "photo".into(),
                            url: "https://pbs.twimg.com/media/a.jpg".into(),
                        },
                        Media {
                            kind: "photo".into(),
                            url: "https://elsewhere.example/a.jpg".into(),
                        },
                        Media {
                            kind: "sticker".into(),
                            url: "https://pbs.twimg.com/media/b.jpg".into(),
                        },
                    ],
                    created_at: "2026-01-01T00:00:00Z".into(),
                    ..Post::default()
                },
                Post {
                    id: "11".into(),
                    author_handle: String::new(),
                    ..Post::default()
                },
            ],
        };
        let (users, posts, dropped) = sanitize(Network::X, batch).expect("ok");
        assert_eq!(users.len(), 1);
        assert_eq!(users[0].network, Network::X);
        assert_eq!(users[0].bio.len(), MAX_BIO);
        assert_eq!(users[0].followers, 0);
        assert_eq!(users[0].avatar, "");
        assert_eq!(users[0].created_at, "");
        assert_eq!(users[0].source, "Following");
        assert_eq!(posts.len(), 1);
        assert_eq!(posts[0].kind, "post");
        assert_eq!(posts[0].reply_to, "");
        assert_eq!(posts[0].media.len(), 1);
        assert_eq!(posts[0].created_at, "2026-01-01T00:00:00Z");
        assert_eq!(dropped, 3);
    }

    #[test]
    fn oversized_batches_are_refused_and_bad_sources_become_unknown() {
        let too_many = Batch {
            source: "X".into(),
            users: (0..=MAX_ITEMS).map(|_| User::default()).collect(),
            posts: vec![],
        };
        assert!(sanitize(Network::X, too_many).is_err());
        let (users, _, _) = sanitize(
            Network::X,
            Batch {
                source: "bad source!".into(),
                users: vec![User {
                    id: "1".into(),
                    handle: "a".into(),
                    ..User::default()
                }],
                posts: vec![],
            },
        )
        .expect("ok");
        assert_eq!(users[0].source, "Unknown");
    }

    #[test]
    fn clipping_never_splits_a_character() {
        let text = "ééééé";
        assert_eq!(clip(text, 3), "é");
        assert_eq!(clip(text, 100), text);
    }
}
