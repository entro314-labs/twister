/**
 * The IPC contract, mirroring the Rust types in `src-tauri/src`.
 *
 * Hand-written rather than generated: the surface is small enough that a codegen step would cost
 * more than it saves. Each block names the Rust struct it mirrors — change one and the other has to
 * follow.
 */

/** `settings::Niceties` — one switch per thing the bridge changes about x.com. */
export interface Niceties {
  chronologicalHome: boolean
  hidePromoted: boolean
  hideRightColumn: boolean
  hideExtrasNav: boolean
  hideViewCounts: boolean
  hideSiteNav: boolean
  dockBadge: boolean
}

/** `settings::Settings` */
export interface Settings {
  theme: 'system' | 'light' | 'dark'
  windowMaterial: 'off' | 'standard' | 'strong'
  niceties: Niceties
}

/** `site::Section` — where on X the page currently is. */
export type Section =
  | 'home'
  | 'explore'
  | 'notifications'
  | 'messages'
  | 'bookmarks'
  | 'profile'
  | 'compose'
  | 'other'

/** `site::Destination` — where the sidebar and the menu can send the site. */
export type Destination =
  | 'home'
  | 'explore'
  | 'notifications'
  | 'messages'
  | 'bookmarks'
  | 'profile'
  | 'compose'

/** `site::Action` */
export type SiteAction = 'back' | 'forward' | 'reload'

/** `site::SiteState`, delivered on `twister://site-state`. */
export interface SiteState {
  url: string
  section: Section
  /** The page title with X's decoration stripped: "(3) Home / X" → "Home". */
  title: string
  unread: number
  loading: boolean
  /** The signed-in handle, once the bridge has seen X's profile link. */
  handle: string | null
}

/** `site::Insets` — the frame around the island, in logical pixels. */
export interface Insets {
  left: number
  top: number
  right: number
  bottom: number
}

/** `site::Notice`, delivered on `twister://notice`. */
export interface Notice {
  message: string
}

/** `menu::ShellAction`, delivered on `twister://shell`. */
export type ShellAction = 'openSettings' | 'toggleSidebar'
