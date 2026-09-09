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

## What it does

- **A native frame.** Sidebar with the six places you actually go, a titlebar
  with back / forward / reload and the page title, a status bar that says where
  on X you are and whether it is still loading. macOS overlay titlebar and
  vibrancy, Windows Mica, app-drawn window controls off macOS.
- **Shortcuts that work whichever view has focus.** ⌘1–⌘6 for Home, Explore,
  Notifications, Messages, Bookmarks and Profile; ⌘N new post; ⌘[ ⌘] ⌘R;
  ⌘\ for the sidebar; ⌘, for settings. They are menu accelerators, which is
  also why ⌘C and ⌘V work at all on macOS.
- **The niceties**, each a switch in Settings:
  - *Following first* — opens the home timeline on Following, once per visit.
  - *Hide promoted posts.*
  - *Hide the right column* — trends, who to follow, premium upsells.
  - *Trim X's navigation* — Grok, Premium, Jobs and the other non-timeline
    entries.
  - *Hide view counts.*
  - *Hide X's navigation entirely* — Twister's sidebar carries the same
    destinations.
  - *Unread count on the Dock icon*, mirrored from the page title.
- **Links leave.** Anything that is not X opens in your browser, and the status
  bar says so. `t.co` redirects are followed to their real target first.
- **Closing hides.** The session stays warm and the badge keeps counting;
  reopen from the Dock, quit with ⌘Q. Window placement is remembered.

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

One window, two child webviews. The **shell** is this app's React page and
owns the frame. The **site** is x.com, added second so it sits above the shell,
positioned by Rust from insets the shell measures — so it follows the sidebar
collapsing and every window resize with no round trip. Anything the shell
needs to show over the island (Settings) hides the site first.

The bridge script runs at document start in the site view and can call exactly
three commands — fetch the niceties, report a navigation, report the signed-in
handle — granted by a capability scoped to x.com's origin and nothing else.
Each validates its input, because anything on that page could call it too.

```
src/                    React 19 + TanStack Router + Tailwind 4
  components/shell/     sidebar, pane titlebar, status bar, window controls
  components/ui/        Base UI primitives with the house chrome
  lib/tauri/            the IPC contract: command registry, client, types
  lib/query/            TanStack Query hooks and the Rust event bridge
  lib/site-island.ts    keeps the site webview glued to the island
  routes/               the island (x.com) and settings
src-tauri/src/
  lib.rs                the window, both webviews, window events
  site.rs               the site webview: allowlist, title parsing, sections,
                        layout, the bridge's three commands
  menu.rs               the application menu and its accelerators
  settings.rs           preferences and window placement as JSON
  commands.rs           every Tauri command
src-tauri/site/
  bridge.js             the script injected into x.com
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

Twister stores two small JSON files in your app data directory: preferences and
window placement. Your X session is a cookie in the site view's own store,
which Twister never reads. Sign out in Settings clears that store. The shell's
content-security policy allows no outbound connections; the only thing that
talks to X is X.
