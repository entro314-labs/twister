//! The networks Twister frames, and everything the rest of the app has to
//! know to tell one from another: where a tab may go, what a handle and an
//! id look like there, which of the six destinations exist, what the
//! composer allows, and which operations Twister is willing to run.
//!
//! Each network is treated on its own terms. X gets everything. Bluesky's
//! web app is a single-page React app with stable test ids, so it gets the
//! operations too, with lower caps. Threads and Instagram are Meta's web
//! stack — generated class names and aggressive automation detection — so
//! Twister only watches those: capture, scan and downloads, and refuses to
//! follow, delete or post on them. Adding a network is a variant here, its
//! site scripts under `site/<slug>/`, and its row in the renderer's table.

use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};
use crate::site::{Destination, Section};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "lowercase")]
pub enum Network {
    #[default]
    X,
    Bluesky,
    Threads,
    Instagram,
}

pub const ALL: &[Network] = &[
    Network::X,
    Network::Bluesky,
    Network::Threads,
    Network::Instagram,
];

/// What the store holds when a row predates networks.
pub const DEFAULT_SLUG: &str = "x";

impl Network {
    pub fn slug(self) -> &'static str {
        match self {
            Self::X => "x",
            Self::Bluesky => "bluesky",
            Self::Threads => "threads",
            Self::Instagram => "instagram",
        }
    }

    pub fn parse(slug: &str) -> Option<Self> {
        ALL.iter().copied().find(|n| n.slug() == slug)
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::X => "X",
            Self::Bluesky => "Bluesky",
            Self::Threads => "Threads",
            Self::Instagram => "Instagram",
        }
    }

    /// The page origin, with no trailing slash. Paths are appended to it.
    pub fn origin(self) -> &'static str {
        match self {
            Self::X => "https://x.com",
            Self::Bluesky => "https://bsky.app",
            Self::Threads => "https://www.threads.com",
            Self::Instagram => "https://www.instagram.com",
        }
    }

    pub fn home(self) -> String {
        format!("{}{}", self.origin(), self.home_path())
    }

    fn home_path(self) -> &'static str {
        match self {
            Self::X => "/home",
            Self::Bluesky | Self::Threads | Self::Instagram => "/",
        }
    }

    /// Exact hosts a tab may open on. The first is the canonical one.
    fn tab_hosts(self) -> &'static [&'static str] {
        match self {
            Self::X => &["x.com", "twitter.com", "www.x.com"],
            Self::Bluesky => &["bsky.app"],
            Self::Threads => &[
                "www.threads.com",
                "threads.com",
                "www.threads.net",
                "threads.net",
            ],
            Self::Instagram => &["www.instagram.com", "instagram.com"],
        }
    }

    /// Hosts a navigation may go to inside one of this network's tabs, matched
    /// as suffixes. `on_navigation` fires for iframes as well as the main
    /// frame, so the sign-in providers each site embeds are here too —
    /// denying them would blank the buttons on the login page.
    fn allowed_hosts(self) -> &'static [&'static str] {
        match self {
            Self::X => &[
                "x.com",
                "twitter.com",
                "twimg.com",
                "t.co",
                "accounts.google.com",
                "appleid.apple.com",
            ],
            // A user's own PDS answers XHR, not navigations; the hosts that
            // a page load can land on are the app, Bluesky's own PDS
            // domains, and the CDNs.
            Self::Bluesky => &["bsky.app", "bsky.social", "bsky.network", "bsky.team"],
            // Threads signs in through Instagram, which signs in through
            // Facebook, and threads.net still redirects to threads.com.
            Self::Threads => &[
                "threads.com",
                "threads.net",
                "instagram.com",
                "facebook.com",
                "cdninstagram.com",
                "fbcdn.net",
            ],
            Self::Instagram => &[
                "instagram.com",
                "facebook.com",
                "cdninstagram.com",
                "fbcdn.net",
                "accounts.google.com",
            ],
        }
    }

    pub fn allows_host(self, host: &str) -> bool {
        self.allowed_hosts()
            .iter()
            .any(|allowed| host == *allowed || host.ends_with(&format!(".{allowed}")))
    }

    /// Where a photo or video may be fetched from: the network's own CDN.
    pub fn media_host_ok(self, host: &str) -> bool {
        let hosts: &[&str] = match self {
            Self::X => &["twimg.com"],
            Self::Bluesky => &["cdn.bsky.app", "video.bsky.app"],
            Self::Threads | Self::Instagram => &["cdninstagram.com", "fbcdn.net"],
        };
        hosts
            .iter()
            .any(|allowed| host == *allowed || host.ends_with(&format!(".{allowed}")))
    }

    /// What a handle looks like here. Anything else from the page is
    /// discarded — the page is not trusted.
    pub fn valid_handle(self, handle: &str) -> bool {
        match self {
            // 1–15 word characters.
            Self::X => {
                !handle.is_empty()
                    && handle.len() <= 15
                    && handle
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'_')
            }
            // A domain name: labels of letters, digits and hyphens, at least
            // two of them, none empty, none starting or ending in a hyphen.
            Self::Bluesky => {
                handle.len() <= 253
                    && handle.contains('.')
                    && handle.split('.').all(|label| {
                        !label.is_empty()
                            && label.len() <= 63
                            && !label.starts_with('-')
                            && !label.ends_with('-')
                            && label
                                .bytes()
                                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
                    })
            }
            // Instagram's rule, which Threads shares: up to 30 of letters,
            // digits, periods and underscores.
            Self::Threads | Self::Instagram => {
                !handle.is_empty()
                    && handle.len() <= 30
                    && handle
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.')
            }
        }
    }

    /// What a person's or a post's id looks like here.
    pub fn valid_id(self, id: &str) -> bool {
        match self {
            // Snowflakes and Meta's pks: digits.
            Self::X | Self::Threads | Self::Instagram => {
                !id.is_empty() && id.len() <= 25 && id.bytes().all(|b| b.is_ascii_digit())
            }
            // A DID (`did:plc:…`, `did:web:…`) for a person, an at-URI
            // (`at://did:…/app.bsky.feed.post/rkey`) for a post.
            Self::Bluesky => {
                id.len() <= 256
                    && (id.starts_with("did:") || id.starts_with("at://did:"))
                    && id.bytes().all(|b| {
                        b.is_ascii_alphanumeric() || matches!(b, b':' | b'.' | b'/' | b'_' | b'-')
                    })
            }
        }
    }

    /// The short id a URL carries when it is not the id itself: Meta's
    /// shortcode. X and Bluesky need none.
    pub fn valid_slug(self, slug: &str) -> bool {
        match self {
            Self::X | Self::Bluesky => slug.is_empty(),
            Self::Threads | Self::Instagram => {
                slug.len() <= 40
                    && slug
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            }
        }
    }

    pub fn profile_url(self, handle: &str) -> String {
        format!("{}{}", self.origin(), self.profile_path(handle))
    }

    fn profile_path(self, handle: &str) -> String {
        match self {
            Self::X => format!("/{handle}"),
            Self::Bluesky => format!("/profile/{handle}"),
            Self::Threads => format!("/@{handle}"),
            Self::Instagram => format!("/{handle}/"),
        }
    }

    pub fn post_url(self, handle: &str, id: &str, slug: &str) -> String {
        match self {
            Self::X => format!("https://x.com/{handle}/status/{id}"),
            Self::Bluesky => {
                let rkey = id.rsplit('/').next().unwrap_or(id);
                format!("https://bsky.app/profile/{handle}/post/{rkey}")
            }
            Self::Threads => format!("https://www.threads.com/@{handle}/post/{slug}"),
            Self::Instagram => format!("https://www.instagram.com/p/{slug}/"),
        }
    }

    /// The composer's limit and how it counts, or `None` where Twister does
    /// not post: Meta's composers are modals in a stack Twister does not
    /// drive.
    pub fn compose_limit(self) -> Option<usize> {
        match self {
            Self::X => Some(280),
            Self::Bluesky => Some(300),
            Self::Threads | Self::Instagram => None,
        }
    }

    /// Which operations run here. Scan is capture with scrolling and is
    /// always fine; the rest click the site's own buttons on the user's
    /// behalf, which Meta's detection treats as a reason to lock an
    /// account.
    pub fn supports(self, kind: &str) -> bool {
        match self {
            Self::X | Self::Bluesky => true,
            Self::Threads | Self::Instagram => kind == "scan",
        }
    }

    /// Why an operation is refused, for the message.
    pub fn refusal(self, kind: &str) -> String {
        format!(
            "Twister does not {} on {}: it only watches Meta's sites. Use {}'s own controls.",
            crate::ops::label(kind).to_lowercase(),
            self.name(),
            self.name()
        )
    }

    /// The decoration each site puts at the end of its page titles, longest
    /// first so the longer form is tried before its prefix.
    pub fn title_suffixes(self) -> &'static [&'static str] {
        match self {
            Self::X => &["/ X", "/ Twitter"],
            Self::Bluesky => &["— Bluesky"],
            Self::Threads => &["• Threads, Say more", "• Threads"],
            Self::Instagram => &["• Instagram photos and videos", "• Instagram"],
        }
    }

    /// The destinations this network has a page for.
    pub fn destinations(self) -> &'static [Destination] {
        match self {
            Self::X | Self::Bluesky => &[
                Destination::Home,
                Destination::Explore,
                Destination::Notifications,
                Destination::Messages,
                Destination::Bookmarks,
                Destination::Profile,
                Destination::Compose,
            ],
            Self::Threads => &[
                Destination::Home,
                Destination::Explore,
                Destination::Notifications,
                Destination::Bookmarks,
                Destination::Profile,
            ],
            Self::Instagram => &[
                Destination::Home,
                Destination::Explore,
                Destination::Messages,
                Destination::Bookmarks,
                Destination::Profile,
            ],
        }
    }

    /// The path a destination lives at, or why it cannot be reached.
    pub fn destination_path(
        self,
        destination: Destination,
        handle: Option<&str>,
    ) -> Result<String> {
        if !self.destinations().contains(&destination) {
            return Err(AppError::InvalidInput(format!(
                "{} has no {} page.",
                self.name(),
                destination.label().to_lowercase()
            )));
        }
        let profile = || {
            handle.map(|h| self.profile_path(h)).ok_or_else(|| {
                AppError::InvalidInput(format!(
                    "Twister has not seen your handle yet. Open {}'s home first.",
                    self.name()
                ))
            })
        };
        Ok(match (self, destination) {
            (_, Destination::Home) => self.home_path().into(),
            (Self::X, Destination::Explore) => "/explore".into(),
            (Self::X | Self::Bluesky, Destination::Notifications) => "/notifications".into(),
            // X moved its messages to /i/chat; /messages still redirects there.
            (Self::X, Destination::Messages) => "/i/chat".into(),
            (Self::X, Destination::Bookmarks) => "/i/bookmarks".into(),
            (Self::X, Destination::Compose) => "/compose/post".into(),
            (Self::Bluesky | Self::Threads, Destination::Explore) => "/search".into(),
            (Self::Bluesky, Destination::Messages) => "/messages".into(),
            (Self::Bluesky | Self::Threads, Destination::Bookmarks) => "/saved".into(),
            (Self::Bluesky, Destination::Compose) => "/intent/compose".into(),
            (Self::Threads, Destination::Notifications) => "/activity".into(),
            (Self::Instagram, Destination::Explore) => "/explore/".into(),
            (Self::Instagram, Destination::Messages) => "/direct/inbox/".into(),
            (Self::Instagram, Destination::Bookmarks) => format!("{}saved/", profile()?),
            (_, Destination::Profile) => profile()?,
            // Every remaining pair was refused above.
            (Self::Threads | Self::Instagram, _) => unreachable!("destination filtered"),
        })
    }

    /// Where on the site a path is. The handle, when known, is what tells
    /// the signed-in profile from anyone else's.
    pub fn section_for(self, path: &str, handle: Option<&str>) -> Section {
        let path = path.trim_end_matches('/');
        let first = path
            .strip_prefix('/')
            .and_then(|rest| rest.split('/').next());
        match self {
            Self::X => match path {
                "/home" | "" => Section::Home,
                "/explore" | "/search" => Section::Explore,
                "/notifications" => Section::Notifications,
                // X redirects /i/bookmarks to /i/history, which is the same page.
                "/i/bookmarks" | "/i/history" => Section::Bookmarks,
                "/compose/post" => Section::Compose,
                _ if path.starts_with("/explore/")
                    || path.starts_with("/search")
                    || path.starts_with("/i/trends") =>
                {
                    Section::Explore
                }
                _ if path.starts_with("/notifications/") => Section::Notifications,
                _ if path.starts_with("/messages") || path.starts_with("/i/chat") => {
                    Section::Messages
                }
                _ if handle.is_some() && first == handle => Section::Profile,
                _ => Section::Other,
            },
            Self::Bluesky => match path {
                "" => Section::Home,
                "/search" => Section::Explore,
                "/notifications" => Section::Notifications,
                "/saved" => Section::Bookmarks,
                "/intent/compose" => Section::Compose,
                _ if path.starts_with("/search") || path.starts_with("/hashtag/") => {
                    Section::Explore
                }
                _ if path.starts_with("/notifications/") => Section::Notifications,
                _ if path.starts_with("/messages") => Section::Messages,
                _ if handle.is_some()
                    && path
                        .strip_prefix("/profile/")
                        .and_then(|rest| rest.split('/').next())
                        == handle =>
                {
                    Section::Profile
                }
                _ => Section::Other,
            },
            Self::Threads => match path {
                "" => Section::Home,
                "/search" => Section::Explore,
                "/activity" => Section::Notifications,
                "/saved" => Section::Bookmarks,
                _ if path.starts_with("/search") => Section::Explore,
                _ if path.starts_with("/activity/") => Section::Notifications,
                _ if handle.is_some() && first.and_then(|f| f.strip_prefix('@')) == handle => {
                    Section::Profile
                }
                _ => Section::Other,
            },
            Self::Instagram => match path {
                "" => Section::Home,
                "/explore" => Section::Explore,
                _ if path.starts_with("/explore/") => Section::Explore,
                _ if path.starts_with("/direct") => Section::Messages,
                _ if handle.is_some() && first == handle => {
                    if path.ends_with("/saved") || path.contains("/saved/") {
                        Section::Bookmarks
                    } else {
                        Section::Profile
                    }
                }
                _ => Section::Other,
            },
        }
    }
}

/// The network whose page a URL is, by its host: what a tab opens as.
pub fn for_tab_url(url: &url::Url) -> Option<Network> {
    if url.scheme() != "https" {
        return None;
    }
    let host = url.host_str()?;
    ALL.iter()
        .copied()
        .find(|network| network.tab_hosts().contains(&host))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(s: &str) -> url::Url {
        url::Url::parse(s).expect("test url")
    }

    #[test]
    fn slugs_round_trip() {
        for network in ALL {
            assert_eq!(Network::parse(network.slug()), Some(*network));
        }
        assert_eq!(Network::parse("myspace"), None);
    }

    #[test]
    fn tabs_open_on_each_networks_own_hosts() {
        assert_eq!(for_tab_url(&url("https://x.com/home")), Some(Network::X));
        assert_eq!(
            for_tab_url(&url("https://twitter.com/home")),
            Some(Network::X)
        );
        assert_eq!(
            for_tab_url(&url("https://bsky.app/")),
            Some(Network::Bluesky)
        );
        assert_eq!(
            for_tab_url(&url("https://www.threads.com/@zuck")),
            Some(Network::Threads)
        );
        assert_eq!(
            for_tab_url(&url("https://threads.net/@zuck")),
            Some(Network::Threads)
        );
        assert_eq!(
            for_tab_url(&url("https://www.instagram.com/instagram/")),
            Some(Network::Instagram)
        );
        assert_eq!(for_tab_url(&url("https://t.co/abc")), None);
        assert_eq!(for_tab_url(&url("https://pbs.twimg.com/a.jpg")), None);
        assert_eq!(for_tab_url(&url("https://cdn.bsky.app/img")), None);
        assert_eq!(for_tab_url(&url("http://x.com/home")), None);
    }

    #[test]
    fn navigation_allowlists_are_per_network() {
        assert!(Network::X.allows_host("api.x.com"));
        assert!(Network::X.allows_host("accounts.google.com"));
        assert!(!Network::X.allows_host("bsky.app"));
        assert!(!Network::X.allows_host("x.com.evil.example"));
        assert!(Network::Bluesky.allows_host("public.api.bsky.app"));
        assert!(Network::Bluesky.allows_host("morel.us-east.host.bsky.network"));
        assert!(!Network::Bluesky.allows_host("x.com"));
        assert!(Network::Threads.allows_host("www.instagram.com"));
        assert!(Network::Threads.allows_host("www.facebook.com"));
        assert!(Network::Instagram.allows_host("scontent.cdninstagram.com"));
        assert!(!Network::Instagram.allows_host("threads.com"));
    }

    #[test]
    fn media_hosts_are_each_networks_cdn() {
        assert!(Network::X.media_host_ok("pbs.twimg.com"));
        assert!(!Network::X.media_host_ok("cdn.bsky.app"));
        assert!(Network::Bluesky.media_host_ok("cdn.bsky.app"));
        assert!(Network::Bluesky.media_host_ok("video.bsky.app"));
        assert!(!Network::Bluesky.media_host_ok("bsky.app"));
        assert!(Network::Threads.media_host_ok("instagram.fath5-1.fna.fbcdn.net"));
        assert!(Network::Instagram.media_host_ok("scontent.cdninstagram.com"));
        assert!(!Network::Instagram.media_host_ok("twimg.com"));
    }

    #[test]
    fn handles_follow_each_networks_rule() {
        assert!(Network::X.valid_handle("dom_314"));
        assert!(!Network::X.valid_handle("sixteen_chars_xx"));
        assert!(!Network::X.valid_handle("has.dot"));
        assert!(Network::Bluesky.valid_handle("alice.bsky.social"));
        assert!(Network::Bluesky.valid_handle("orchardnotes.com"));
        assert!(!Network::Bluesky.valid_handle("alice"));
        assert!(!Network::Bluesky.valid_handle("alice..com"));
        assert!(!Network::Bluesky.valid_handle("-alice.com"));
        assert!(!Network::Bluesky.valid_handle("did:plc:abc"));
        assert!(Network::Threads.valid_handle("zuck"));
        assert!(Network::Instagram.valid_handle("tom.developer"));
        assert!(Network::Instagram.valid_handle("ai_work_flows_"));
        assert!(!Network::Instagram.valid_handle("a".repeat(31).as_str()));
        assert!(!Network::Threads.valid_handle("has space"));
        assert!(!Network::Threads.valid_handle(""));
    }

    #[test]
    fn ids_follow_each_networks_rule() {
        assert!(Network::X.valid_id("1234567890"));
        assert!(!Network::X.valid_id("did:plc:z72i7hdynmk6r22z27h6tvur"));
        assert!(Network::Bluesky.valid_id("did:plc:z72i7hdynmk6r22z27h6tvur"));
        assert!(Network::Bluesky.valid_id("did:web:example.com"));
        assert!(
            Network::Bluesky
                .valid_id("at://did:plc:z72i7hdynmk6r22z27h6tvur/app.bsky.feed.post/3l6oveex3ii2l")
        );
        assert!(!Network::Bluesky.valid_id("3l6oveex3ii2l"));
        assert!(!Network::Bluesky.valid_id("at://did:plc:x/app.bsky.feed.post/a b"));
        assert!(Network::Threads.valid_id("3987048990160038468"));
        assert!(!Network::Threads.valid_id("3987048990160038468_63055343223"));
        assert!(Network::Threads.valid_slug("DdU1-6okapE"));
        assert!(Network::Instagram.valid_slug("DdEo7DPG8x2"));
        assert!(!Network::Instagram.valid_slug("has/slash"));
        assert!(Network::X.valid_slug(""));
        assert!(!Network::X.valid_slug("abc"));
    }

    #[test]
    fn urls_are_built_per_network() {
        assert_eq!(Network::X.profile_url("dom"), "https://x.com/dom");
        assert_eq!(
            Network::Bluesky.profile_url("bsky.app"),
            "https://bsky.app/profile/bsky.app"
        );
        assert_eq!(
            Network::Threads.profile_url("zuck"),
            "https://www.threads.com/@zuck"
        );
        assert_eq!(
            Network::Instagram.profile_url("instagram"),
            "https://www.instagram.com/instagram/"
        );
        assert_eq!(
            Network::X.post_url("alice", "10", ""),
            "https://x.com/alice/status/10"
        );
        assert_eq!(
            Network::Bluesky.post_url(
                "bsky.app",
                "at://did:plc:z72i7hdynmk6r22z27h6tvur/app.bsky.feed.post/3l6oveex3ii2l",
                ""
            ),
            "https://bsky.app/profile/bsky.app/post/3l6oveex3ii2l"
        );
        assert_eq!(
            Network::Threads.post_url("zuck", "398", "DdU1-6okapE"),
            "https://www.threads.com/@zuck/post/DdU1-6okapE"
        );
        assert_eq!(
            Network::Instagram.post_url("instagram", "398", "DdEo7DPG8x2"),
            "https://www.instagram.com/p/DdEo7DPG8x2/"
        );
    }

    #[test]
    fn destinations_exist_per_network_and_profile_needs_a_handle() {
        assert_eq!(
            Network::X
                .destination_path(Destination::Home, None)
                .expect("ok"),
            "/home"
        );
        assert_eq!(
            Network::Bluesky
                .destination_path(Destination::Bookmarks, None)
                .expect("ok"),
            "/saved"
        );
        assert!(
            Network::X
                .destination_path(Destination::Profile, None)
                .is_err()
        );
        assert_eq!(
            Network::X
                .destination_path(Destination::Profile, Some("dom"))
                .expect("ok"),
            "/dom"
        );
        assert_eq!(
            Network::Instagram
                .destination_path(Destination::Bookmarks, Some("dom"))
                .expect("ok"),
            "/dom/saved/"
        );
        assert!(
            Network::Instagram
                .destination_path(Destination::Notifications, Some("dom"))
                .is_err()
        );
        assert!(
            Network::Threads
                .destination_path(Destination::Messages, Some("dom"))
                .is_err()
        );
        assert!(
            Network::Threads
                .destination_path(Destination::Compose, Some("dom"))
                .is_err()
        );
        assert!(Network::Bluesky.compose_limit().is_some());
        assert!(Network::Instagram.compose_limit().is_none());
        assert!(Network::Threads.supports("scan"));
        assert!(!Network::Threads.supports("follow"));
        assert!(Network::Bluesky.supports("delete"));
    }

    #[test]
    fn sections_follow_paths_and_the_known_handle() {
        use Network::{Bluesky, Instagram, Threads, X};
        assert_eq!(X.section_for("/home", None), Section::Home);
        assert_eq!(X.section_for("/", None), Section::Home);
        assert_eq!(X.section_for("/explore/tabs/news", None), Section::Explore);
        assert_eq!(X.section_for("/search", None), Section::Explore);
        assert_eq!(
            X.section_for("/notifications/mentions", None),
            Section::Notifications
        );
        assert_eq!(X.section_for("/messages/123", None), Section::Messages);
        assert_eq!(X.section_for("/i/chat", None), Section::Messages);
        assert_eq!(X.section_for("/i/history", None), Section::Bookmarks);
        assert_eq!(X.section_for("/compose/post", None), Section::Compose);
        assert_eq!(X.section_for("/dom", None), Section::Other);
        assert_eq!(X.section_for("/dom", Some("dom")), Section::Profile);
        assert_eq!(
            X.section_for("/dom/with_replies", Some("dom")),
            Section::Profile
        );
        assert_eq!(X.section_for("/someone", Some("dom")), Section::Other);

        assert_eq!(Bluesky.section_for("/", None), Section::Home);
        assert_eq!(Bluesky.section_for("/search?q=a", None), Section::Explore);
        assert_eq!(
            Bluesky.section_for("/hashtag/Emmys", None),
            Section::Explore
        );
        assert_eq!(Bluesky.section_for("/saved", None), Section::Bookmarks);
        assert_eq!(
            Bluesky.section_for("/messages/abc", None),
            Section::Messages
        );
        assert_eq!(
            Bluesky.section_for("/profile/a.bsky.social", Some("a.bsky.social")),
            Section::Profile
        );
        assert_eq!(
            Bluesky.section_for("/profile/a.bsky.social/post/3k", Some("a.bsky.social")),
            Section::Profile
        );
        assert_eq!(
            Bluesky.section_for("/profile/b.bsky.social", Some("a.bsky.social")),
            Section::Other
        );

        assert_eq!(Threads.section_for("/", None), Section::Home);
        assert_eq!(
            Threads.section_for("/activity", None),
            Section::Notifications
        );
        assert_eq!(
            Threads.section_for("/@zuck", Some("zuck")),
            Section::Profile
        );
        assert_eq!(
            Threads.section_for("/@zuck/post/abc", Some("zuck")),
            Section::Profile
        );
        assert_eq!(Threads.section_for("/@muse", Some("zuck")), Section::Other);

        assert_eq!(Instagram.section_for("/", None), Section::Home);
        assert_eq!(Instagram.section_for("/explore/", None), Section::Explore);
        assert_eq!(
            Instagram.section_for("/direct/inbox/", None),
            Section::Messages
        );
        assert_eq!(
            Instagram.section_for("/dom/", Some("dom")),
            Section::Profile
        );
        assert_eq!(
            Instagram.section_for("/dom/saved/", Some("dom")),
            Section::Bookmarks
        );
        assert_eq!(
            Instagram.section_for("/p/abc/", Some("dom")),
            Section::Other
        );
    }
}
