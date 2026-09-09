# Changelog

All notable changes to Twister are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Three niceties.** *Hide the floating drawers* (Grok and Messages in the
  corner); *The bird* (the blue bird for the X mark, classic blue on Post and
  Follow); *Dim*, X's retired blue-grey theme, rebuilt over Lights out by
  reading X's own stylesheet and shadowing every rule that paints a Lights-out
  colour — no class names of X's are known to the app.
- **User scripts and styles.** `scripts/*.js` and `styles/*.css` in the app
  data directory are injected into every X page as initialization scripts,
  outside X's content-security policy. Settings lists them, opens the folder,
  and reloads the site view to pick up changes.
- **A tooltip layer.** Tooltips are a tiny child window of their own, so they
  float above the site view instead of vanishing under it, and carry the
  app's theme and the shortcut.

### Changed

- **Navigation stays inside X's app.** The sidebar, the View menu and New
  post now click X's own links rather than reloading the page, so a
  destination is instant and the compose modal opens at all — loaded cold,
  `/compose/post` sat on X's splash screen for good. Bookmarks, which X keeps
  off its navigation, still loads in full.
- **Promoted posts** are caught by a second net: X's own "Ad" marker, outside
  video-player chrome, hides its timeline cell.
- **The right column stays in Messages**, where it is the conversation.
- **Grok** is found by its route and its glyph rather than one English label,
  and its in-post follow-up chips go with it.
- **"Following first"** falls back to the second tab by position where the UI
  is not English.
- **The collapsed sidebar** is wide enough on macOS for the traffic lights,
  so its toggle sits on the same axis as the icons under it.
- Messages moved with X to `/i/chat`.

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
