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
  hideDrawers: boolean
  classicTwitter: boolean
  dim: boolean
  capture: boolean
  downloadButton: boolean
  fitTimeline: boolean
  smoothScroll: boolean
  compactPosts: boolean
  squareAvatars: boolean
  actionsOnHover: boolean
  hideActionCounts: boolean
  starFavorites: boolean
  compactCompose: boolean
  hideInlineComposer: boolean
  hidePageHeaders: boolean
  hideTimelineModules: boolean
  timeOnRight: boolean
  mediaThumbnails: boolean
}

/** `settings::Settings` */
export interface Settings {
  theme: 'system' | 'light' | 'dark'
  windowMaterial: 'off' | 'standard' | 'strong'
  niceties: Niceties
  /** A CSS font-family for X's text, or empty for X's own. */
  font: string
  /** The size of a post's text. */
  textSize: 'small' | 'normal' | 'large'
}

/** `site::Section` — where on X a tab currently is. */
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

/** `site::TabState` — one open tab. */
export interface TabState {
  id: number
  url: string
  section: Section
  /** The page title with X's decoration stripped: "(3) Home / X" → "Home". */
  title: string
  unread: number
  loading: boolean
}

/** `site::SiteState`, delivered on `twister://site-state`. */
export interface SiteState {
  tabs: TabState[]
  /** The id of the tab in front; 0 before any tab exists. */
  active: number
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
export type ShellAction =
  | 'openSettings'
  | 'toggleSidebar'
  | 'openPeople'
  | 'openPosts'
  | 'openCompose'

/** `userland::UserAsset` — one file in the scripts or styles folder. */
export interface UserAsset {
  name: string
  bytes: number
}

/** `userland::UserAssets` */
export interface UserAssets {
  scripts: UserAsset[]
  styles: UserAsset[]
  dir: string
}

/** `tooltip::Side` */
export type TooltipSide = 'right' | 'bottom'

// ─── The store ──────────────────────────────────────────────────────────────

/** `db::User` — one person, as the page last showed them. */
export interface Person {
  id: string
  handle: string
  name: string
  bio: string
  location: string
  website: string
  followers: number
  following: number
  posts: number
  verified: boolean
  protected: boolean
  avatar: string
  createdAt: string
  /** They follow the signed-in account; `null` when X did not say. */
  followsMe: boolean | null
  /** The signed-in account follows them; `null` when X did not say. */
  followedByMe: boolean | null
  /** The X operation that loaded them: Following, Followers, ListMembers… */
  source: string
  firstSeen: string
  lastSeen: string
}

/** `db::Media` */
export interface Media {
  /** `photo` | `video` | `animated_gif` */
  kind: string
  url: string
}

export type PostKind = 'post' | 'reply' | 'repost' | 'quote'

/** `db::Post` */
export interface Post {
  id: string
  authorId: string
  authorHandle: string
  text: string
  createdAt: string
  kind: PostKind
  lang: string
  likes: number
  reposts: number
  replies: number
  views: number
  bookmarked: boolean
  media: Media[]
  repostOf: string
  replyTo: string
  quotedId: string
  source: string
  firstSeen: string
  lastSeen: string
}

/** `db::UserFilter` — every field optional; an empty filter is everyone. */
export interface PersonFilter {
  search?: string
  source?: string
  followsMe?: boolean
  followedByMe?: boolean
  verified?: boolean
  minFollowers?: number
  maxFollowers?: number
  minPosts?: number
  sort?: 'seen' | 'followers' | 'handle'
  limit?: number
}

/** `db::PostFilter` */
export interface PostFilter {
  search?: string
  source?: string
  kind?: PostKind
  author?: string
  bookmarked?: boolean
  hasMedia?: boolean
  since?: string
  until?: string
  sort?: 'seen' | 'created' | 'likes'
  limit?: number
}

/** `db::Counts` */
export interface StoreCounts {
  users: number
  posts: number
  /** `[source, rows]` pairs, most rows first. */
  sources: Array<[string, number]>
}

export type ExportFormat = 'csv' | 'json' | 'markdown'

// ─── Operations ─────────────────────────────────────────────────────────────

export type OpKind = 'scan' | 'follow' | 'unfollow' | 'delete' | 'compose'

export type JobStatus = 'queued' | 'running' | 'done' | 'failed' | 'cancelled'

/** `db::Job` — one entry in the ledger. */
export interface Job {
  id: number
  kind: OpKind
  /** JSON, as given to the runner. */
  params: string
  status: JobStatus
  dryRun: boolean
  total: number
  done: number
  skipped: number
  failed: number
  message: string
  origin: 'app' | 'mcp' | 'schedule'
  createdAt: string
  finishedAt: string
}

/** `ops::OpsState` */
export interface OpsState {
  running: Job | null
  recent: Job[]
}

/** `compose::Part` */
export interface PostPart {
  text: string
  count: number
  /** X would link something in it, which the API bills at its own rate. */
  hasLink: boolean
}

/** `compose::Prepared` */
export interface PreparedPost {
  parts: PostPart[]
  limit: number
}

export type ScheduledStatus = 'scheduled' | 'posting' | 'posted' | 'failed' | 'missed'

/** `db::ScheduledPost` */
export interface ScheduledPost {
  id: number
  parts: string[]
  scheduledAt: string
  status: ScheduledStatus
  error: string
  createdAt: string
}
