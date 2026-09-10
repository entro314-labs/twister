# Twister

A desktop client for X (Twitter) with niceties injected. Tauri 2 + Rust +
React 19, based on the [Windbag](https://github.com/entro314-labs/yapper)
shell: a native frame — sidebar, titlebar, status bar, vibrancy, menu and
shortcuts — around x.com itself, running in its own webview with a bridge
script that quietly fixes the things X gets wrong.

## Why a wrapper and not an API client

X retired its tiered API plans in February 2026 and bills every call against
purchased credits; there is no free tier, and reading a home timeline is not an
"owned read". A client that fetched timelines through the official API would
cost its user money on every scroll. So Twister does not touch the API at all.
The page in the island is X's own site, signed in with X's own session, and
Twister frames it. Nothing you read or write passes through Twister.

What that saves, from X's own card (read off docs.x.com on 2026-09-10; it has
moved twice this year):

| What a classic client did          | X's card                    | Price              |
| ---------------------------------- | --------------------------- | ------------------ |
| Refresh the home timeline          | Posts: Read                 | $0.005 a post      |
| Open a post and its replies        | Posts: Read                 | $0.005 a post      |
| Open a profile                     | User: Read                  | $0.010 a profile   |
| Someone else's followers/following | Following/Followers: Read   | $0.010 a person    |
| Search                             | Posts: Read                 | $0.005 a post      |
| Mentions, your own posts, bookmarks| Owned read                  | $0.001 a resource  |
| Read direct messages               | DM Event: Read              | $0.010 a message   |
| Send a direct message              | DM Interaction: Create      | $0.015 a message   |
| Post or reply                      | Post: Create                | $0.015 a post      |
| Post with a link in it             | Post: Create (with URL)     | $0.200 a post      |
| Bookmark                           | Bookmark                    | $0.005 a request   |
| Like, quote, follow, unfollow      | Enterprise only since 2026-04-20 | —             |
| Repost, delete                     | Not named on the card       | —                  |

Reads are billed per resource returned and deduplicated within one UTC day; a
month is capped at three million post reads. An ordinary day of reading — a
few hundred distinct timeline posts, a handful of profiles and threads, some
search, the mentions, a few messages and posts — comes to about $3.50, or a
little over $100 a month, before the likes and follows the API no longer
sells to individuals.
Settings shows the same table with the day's arithmetic; the Write panel says
what each thread would have cost. The numbers live in `src/lib/api-costs.ts`.

## What it does

- **A native frame.** Sidebar with the six places you actually go, a titlebar
  with back / forward / reload and the tabs, a status bar that says where
  on X you are and whether it is still loading. macOS overlay titlebar and
  vibrancy, Windows Mica, app-drawn window controls off macOS.
- **Tabs.** Each tab is an X page in its own webview, sharing the session.
  ⌘T opens one, ⌘W or a middle-click closes it, Ctrl⇥ moves along; popups
  open as tabs; the open tabs come back on the next launch.
- **Shortcuts that work whichever view has focus.** ⌘1–⌘6 for Home, Explore,
  Notifications, Messages, Bookmarks and Profile; ⌘N new post; ⌘[ ⌘] ⌘R;
  ⌘\ for the sidebar; ⌘⇧P ⌘⇧O ⌘⇧N for the tools; ⌘, for settings. They are
  menu accelerators, which is also why ⌘C and ⌘V work at all on macOS.
- **The store, and the tools over it.** While Twister watches, everything X
  loads into a tab — the people on a list, the posts on a timeline, your
  bookmarks — is read from X's own responses as the page receives them and
  kept in a local SQLite file. Nothing is fetched on X's behalf; nothing
  leaves the machine. Three panels open beside the island:
  - *People* — filter by bio, source, follow-back status and follower counts;
    export CSV, JSON or Markdown; scan a list page to its end; follow or
    unfollow a selection on the page that holds them, one every few seconds,
    dry run by default, stopping at the first thing X refuses.
  - *Posts* — the same for posts, bookmarks included; delete your own in
    bulk through X's own menu, dry run by default.
  - *Write* — Markdown in, a thread out, with X's weighted count per part;
    post now through X's composer, or schedule it for while the app is open.
- **Downloads.** A button in each post's action bar saves its photos at full
  size or its video at the best bitrate to Downloads/Twister.
- **The agent door.** `twister-mcp` is an MCP server over the same store:
  `claude mcp add twister -- /path/to/twister-mcp`. Search, export, queue a
  scan or a follow run the app performs next, schedule posts.
- **The niceties**, each a switch in Settings:
  - *Following first* — opens the home timeline on Following, once per visit.
  - *Hide promoted posts.*
  - *Hide the right column* — trends, who to follow, premium upsells.
  - *Trim X's navigation* — Grok, Premium, Jobs and the other non-timeline
    entries.
  - *Hide view counts.*
  - *Hide X's navigation entirely* — Twister's sidebar carries the same
    destinations.
  - *Hide the floating drawers* — the Grok and Messages panels in the corner.
  - *Twitter* — one switch: the blue bird in place of the X mark, the classic
    blue on Post and Follow, and posts called tweets again in X's own
    controls and titles (English only).
  - *Fit the timeline* — X's 600px column grows to fill the island, and the
    window is kept wide enough for X's layout, right column included when it
    shows.
  - *Smooth scrolling*, and a font of your choosing for X's text.
- **The look**, for those who miss a classic client, each its own switch:
  a text size; *Compact posts* (a 32px avatar, a quieter byline, the action
  bar pulled up under the text); *Rounded-square avatars*; *Actions on hover* (the action bar shows
  for the pointer or the keyboard and stays out of the way otherwise);
  *No counts on the action bar*; *Stars, not hearts* (a gold star for the
  like button, and likes called favorites in X's controls); *A quieter
  composer* (no audience chip, no who-can-reply line, no Grok, and a number
  of characters left — from X's own count — in place of the ring); *No
  composer in the timeline*; *Hide X's page headers* (the sticky title band —
  Home and its two tabs, the back arrow on a post — while headers that carry
  tabs or a search field elsewhere stay, and so does the new-posts pill);
  *Only posts in the timeline* (no "Who to follow", "Discover more", news or
  premium blocks on Home, profiles, lists and bookmarks; under a post,
  "Discover more" and all after it); *Time on the right* of the byline;
  *Media as thumbnails* (a 180px cropped strip in timelines, full size on
  the post's page). A *System* button beside the font field sets the OS's
  own face.
  - *Remember what X loads* and *Download button on posts*.
  - *Dim* — X's retired blue-grey dark theme, painted over Lights out. Built
    by reading X's own stylesheet for every rule that paints a Lights-out
    colour and shadowing it, so it needs no class names of X's.
  - *Unread count on the Dock icon*, mirrored from the page title.
- **Your own scripts and styles.** Two folders in the app data directory,
  `scripts/` and `styles/`. Every `*.js` runs on every X page once the DOM is
  ready, the way a Tampermonkey script does (minus the `GM_*` API); every
  `*.css` is applied at document start. Injected as initialization scripts,
  which X's content-security policy cannot block. Settings lists what is in
  the folders, opens them, and reloads.
- **Tooltips that float.** The site view sits above the frame, so a tooltip
  drawn by the frame would vanish under it. Tooltips are a tiny child window
  of their own, styled with the app's theme and drawn by the OS above
  everything.
- **Links leave.** Anything that is not X opens in your browser, and the status
  bar says so. `t.co` redirects are followed to their real target first.
- **Closing hides.** The session stays warm and the badge keeps counting;
  reopen from the Dock, quit with ⌘Q. Window placement is remembered.

## Where the ideas came from

The niceties borrow from the extension world, adapted rather than copied:
zen-view-for-x's two-net promoted-post detection and its Messages exemption
for the right column; TwitterBirdIsBack's bird path, blue buttons, drawer
selectors and locale-independent Grok detection by route and glyph; the
positional "Following" tab fallback from hummingbird. Nothing here calls X's
private API.

## The one honest caveat

Every nicety is a selector on X's own DOM, which X changes without notice. When
one stops matching, that nicety silently does nothing — never breaks the page —
and can be switched off in Settings until the next release. All of the
selectors live in two files, `src-tauri/site/niceties.css` and
`src-tauri/site/bridge.js`, so a fix is a one-file change. "Following first"
finds the tab by its English label.

Sign-in works with X's own form. "Continue with Google" and "Continue with
Apple" open as popups on the web, which a single-view client cannot host; they
are loaded in place instead and may not complete. Use the password form.

## How it is built

One window, child webviews. The **shell** is this app's React page and owns
the frame. Each **tab** is x.com in a webview of its own, added after the
shell so it sits above it, positioned by Rust from insets the shell measures —
so it follows the sidebar collapsing, a tool panel opening and every window
resize with no round trip. Only the front tab is shown. Anything the shell
needs to show over the island (Settings) hides the site first.

Three scripts run at document start in every tab: the bridge (niceties,
navigation, the handle, a layout measurement), the capture hook (X's own API
responses, read as they arrive, batched to the store) and the operations
(scan, follow, unfollow, delete, compose — all clicking what X drew and
reporting progress). Together they may call seven commands, granted by a
capability scoped to x.com's origin and nothing else, and each validates its
input, because anything on that page could call it too. Operations run one at
a time, in the front tab, and every destructive one is a dry run unless told
otherwise.

```
src/                    React 19 + TanStack Router + Tailwind 4
  components/shell/     sidebar, titlebar, tab strip, status bar, window controls
  components/tools/     what the three tool panels share
  components/ui/        Base UI primitives with the house chrome
  lib/tauri/            the IPC contract: command registry, client, types
  lib/query/            TanStack Query hooks and the Rust event bridge
  lib/site-island.ts    keeps the site webview glued to the island
  routes/               the island (x.com), settings, tools/{people,posts,compose}
src-tauri/src/
  lib.rs                the window, the webviews, window events
  site.rs               the tabs: allowlist, title parsing, sections, layout,
                        the bridge's commands
  db.rs                 the store: people, posts, jobs, scheduled posts (SQLite)
  capture.rs            what the page sends the store, and its checks
  ops.rs                operations: one at a time, in the front tab
  compose.rs            Markdown to X-shaped text, the weighted count, the split
  scheduler.rs          due posts and queued jobs, every twenty seconds
  download.rs           media to Downloads/Twister
  export.rs             CSV, JSON and Markdown writers
  mcp.rs, bin/mcp.rs    the agent door
  menu.rs               the application menu and its accelerators
  settings.rs           preferences, window placement and open tabs as JSON
  commands.rs           every Tauri command
src-tauri/site/
  bridge.js             niceties, navigation, the handle, layout
  capture.js            the fetch/XHR hook and the download button
  ops.js                scan, follow, unfollow, delete, compose
  niceties.css          the selectors, gated on attributes the bridge stamps
src-tauri/capabilities/ what each webview may call
```

## Development

```sh
pnpm install
pnpm tauri:dev          # the app, with the Vite dev server
pnpm check              # lint, format, types, build, clippy, rustfmt, rust tests
```

Requires Node 24+, Rust 1.98 and pnpm 12 — `mise install` picks all three up
from `mise.toml` and `rust-toolchain.toml`.

## Privacy

Twister stores three small JSON files in your app data directory — preferences,
window placement, open tabs — and one SQLite file, the store: the people and
posts X loaded into your tabs, kept only when *Remember what X loads* is on and
cleared from Settings. Your X session is a cookie in the site view's own store,
which Twister never reads. Sign out in Settings clears that store. The shell's
content-security policy allows no outbound connections; the only thing that
talks to X is X.
