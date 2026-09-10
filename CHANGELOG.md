# Changelog

All notable changes to Twister are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Tabs.** Every tab is an X page in its own webview, sharing the session;
  only the front one is shown. Arc-style pills in the titlebar,
  middle-click or ⌘W to close, ⌘T for a new one, Ctrl⇥ / Ctrl⇧⇥ to move
  between them, popups open as tabs, and the open tabs come back on the next
  launch. Closing the last tab hides the window, the way a browser does.
- **The store.** While Twister watches, everything X loads into a tab — the
  people on a followers or following list, the posts on a timeline, your
  bookmarks — is read from X's own responses as the page receives them and
  kept in a local SQLite file. No request is made on X's behalf; nothing
  leaves the machine. Settings shows the counts and clears it.
- **People.** A panel beside the island: everyone the store has seen, filtered
  by words in the bio, where they were seen, whether they follow back, and
  follower counts; export as CSV, JSON or Markdown; *Scan this page* scrolls
  the front tab to its end so a whole list is captured; follow or unfollow a
  selection on the list page that holds them, one every few seconds, dry run
  by default, stopping at the first thing X refuses.
- **Posts.** The same for posts: filter by words, kind, author, media and
  where they were seen — Bookmarks included — export, and delete your own in
  bulk through X's own menu (reposts undone, likes unliked on the Likes page),
  dry run by default.
- **Write.** Markdown in, a thread out: bold, italic and code as styled
  letters, lists as bullets, `---` as a thread break, and anything over 280
  split at a sentence, with X's own weighted count per part. Post now types
  it into X's composer; schedule it and Twister posts it while open, marking
  it missed rather than sending it hours late.
- **Downloads.** A button in each post's action bar saves its photos at full
  size or its video at the best bitrate to Downloads/Twister.
- **The agent door.** `twister-mcp`, an MCP server over the same store:
  search people and posts, export, queue a scan, follow, unfollow or delete
  job the app runs next, and schedule posts. Dry runs unless told otherwise.
- **Niceties.** *Twitter* in one switch (the bird, the blue, and posts called
  tweets again in X's own controls and titles); *Fit the timeline*, which lets
  X's 600px column fill the island and keeps the window wide enough for X's
  layout, right column included when it shows; *Smooth scrolling*; a font of
  your choosing for X's text; *Remember what X loads* and *Download button*.
- **A Tools menu** with ⌘⇧P, ⌘⇧O and ⌘⇧N for People, Posts and Write.
- **The look.** A section in Settings for those who miss a classic client,
  each its own switch: a text size for posts; *Compact posts* — a 32px
  avatar, a quieter byline, the action bar pulled up under the text;
  *Rounded-square avatars*; *Actions on hover* — a post's action bar shows
  for the pointer or the keyboard and stays out of the way otherwise, the
  post on its own page excepted; *No counts on the action bar*; *Stars, not
  hearts* — a gold star for the like button, and likes called favorites in
  X's own controls; *A quieter composer* — the audience chip, the
  who-can-reply line and Grok go, and the count is a number of characters
  left, counted by the same rule as the Write panel, in place of the ring;
  *No composer in the timeline*; *Hide X's page headers* — the sticky title
  band, Home's two tabs included, while headers that carry tabs or a search
  field elsewhere stay and the new-posts pill is left standing; *Only posts
  in the timeline* — "Who to follow", "Discover more", news and premium
  blocks go on Home, profiles, lists and bookmarks, and under a post
  "Discover more" takes everything after it; *Time on the right* of the
  byline, the dot before it gone.
- **What the API would charge.** A table in Settings of what X's pay-per-use
  card bills for each thing a classic client did — timeline, threads,
  profiles, search, mentions, messages, posts, bookmarks — with an ordinary
  day's arithmetic against Twister's $0, and a note of what the card no
  longer sells to individuals (likes, follows, quote posts, since April 2026).
  The Write panel says what each thread would have cost, a link counted at
  its own rate.

### Changed

- **The macOS traffic lights** sit where macOS puts them again, centred in
  their band on every version. Twister used to move them itself, which macOS
  26 quietly stopped honouring vertically — leaving them riding high against
  the sidebar's header. The window now carries an empty toolbar and lets the
  system place them; the header band grew to 52px to match, so its title and
  the lights share one centre line.
- **The titlebar** is one band: the browser verbs, the tabs — each carrying
  its page's title — New post as a single icon, and the window controls, with
  a gutter at either end. All of it moves the window.
- **Sidebar icons** are a step larger.
- Bookmarks follows X to `/i/history`.
- *The bird* became *Twitter*; an old settings file still reads.

### Earlier in this cycle

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
