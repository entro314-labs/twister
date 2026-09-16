import {
  IconBrandBluesky,
  IconBrandInstagram,
  IconBrandThreads,
  IconBrandX,
} from '@tabler/icons-react'
import type * as React from 'react'

import type { Destination, Network, OpKind } from '@/lib/tauri/types'

/**
 * What the shell knows about each network, mirroring `network.rs`: the name, the glyph, which of
 * the six destinations its site has a page for, whether Twister posts there, and which operations
 * it is willing to run. Threads and Instagram are watched, never driven — Meta's sites treat a
 * clicked-for-you follow as a reason to lock an account — so they take scans only. Change one side
 * and the other has to follow.
 */
export interface NetworkInfo {
  name: string
  icon: React.ElementType
  destinations: readonly Destination[]
  /** Whether the Write panel can post here; the limit comes back from Rust. */
  composes: boolean
  ops: readonly OpKind[]
  /** The list page the follow tools default to, given the signed-in handle. */
  followingPage: (handle: string) => string
  /** The profile page the delete tool works on, given the signed-in handle. */
  profilePage: (handle: string) => string
  /** A path is one of the network's people lists. */
  isListPage: (path: string) => boolean
}

const ALL_DESTINATIONS: readonly Destination[] = [
  'home',
  'explore',
  'notifications',
  'messages',
  'bookmarks',
  'profile',
  'compose',
]
const ALL_OPS: readonly OpKind[] = ['scan', 'follow', 'unfollow', 'delete', 'compose']

export const NETWORKS: Record<Network, NetworkInfo> = {
  x: {
    name: 'X',
    icon: IconBrandX,
    destinations: ALL_DESTINATIONS,
    composes: true,
    ops: ALL_OPS,
    followingPage: (handle) => `/${handle}/following`,
    profilePage: (handle) => `/${handle}`,
    isListPage: (path) => /\/(?:following|followers|members|verified_followers)/.test(path),
  },
  bluesky: {
    name: 'Bluesky',
    icon: IconBrandBluesky,
    destinations: ALL_DESTINATIONS,
    composes: true,
    ops: ALL_OPS,
    followingPage: (handle) => `/profile/${handle}/follows`,
    profilePage: (handle) => `/profile/${handle}`,
    isListPage: (path) => /^\/profile\/[^/]+\/(?:follows|followers|known-followers)/.test(path),
  },
  threads: {
    name: 'Threads',
    icon: IconBrandThreads,
    destinations: ['home', 'explore', 'notifications', 'bookmarks', 'profile'],
    composes: false,
    ops: ['scan'],
    followingPage: (handle) => `/@${handle}`,
    profilePage: (handle) => `/@${handle}`,
    isListPage: () => false,
  },
  instagram: {
    name: 'Instagram',
    icon: IconBrandInstagram,
    destinations: ['home', 'explore', 'messages', 'bookmarks', 'profile'],
    composes: false,
    ops: ['scan'],
    followingPage: (handle) => `/${handle}/following/`,
    profilePage: (handle) => `/${handle}/`,
    isListPage: (path) => /\/(?:following|followers)\/?$/.test(path),
  },
}

/** The networks in the order the sidebar and the Network menu show them: ⌘⇧1 to ⌘⇧4. */
export const NETWORK_ORDER: readonly Network[] = ['x', 'bluesky', 'threads', 'instagram']

/** The networks the Write panel can post to. */
export const COMPOSING_NETWORKS: readonly Network[] = NETWORK_ORDER.filter(
  (network) => NETWORKS[network].composes,
)
