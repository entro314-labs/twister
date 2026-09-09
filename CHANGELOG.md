# Changelog

All notable changes to Twister are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-09-10

### Added

- **The frame.** One window carrying two child webviews: Twister's own shell
  and x.com. Sidebar with Home, Explore, Notifications, Messages, Bookmarks and
  Profile; a titlebar with back, forward, reload, the page title and New post;
  a status bar with the current section, path, loading state and one-line
  notices. macOS overlay titlebar with vibrancy, Mica on Windows, app-drawn
  window controls off macOS. Window placement is remembered across launches.
- **The niceties**, injected into x.com and each switchable in Settings:
  Following first, hide promoted posts, hide the right column, trim X's
  navigation, hide view counts, hide X's navigation entirely, unread count on
  the Dock icon.
- **Menu and shortcuts.** ⌘1–⌘6 for the six destinations, ⌘N new post, ⌘[ ⌘]
  ⌘R, ⌘\ to collapse the sidebar, ⌘, for settings. Accelerators fire whichever
  webview has focus.
- **Links leave.** Non-X navigations and popups open in the system browser,
  with `t.co` redirects followed to their target first. X's sign-in provider
  frames are allowed so the login page draws correctly.
- **Sign out** in Settings, which clears the site view's cookies and storage.
- A bridge capability that lets x.com call exactly three validated commands
  and nothing else.
