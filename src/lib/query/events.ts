import type { QueryClient } from '@tanstack/react-query'

import { subscribeEvent } from '@/lib/tauri/client'
import { IPC_EVENTS } from '@/lib/tauri/ipc'
import type { SiteState } from '@/lib/tauri/types'

import { queryKeys } from './keys'

/**
 * Bridges Rust's site-state pushes onto the query cache. The state changes when x.com does — a
 * navigation, a new title, an unread count — so it is written straight into the cache rather than
 * refetched: the payload IS the answer.
 *
 * Returns a detach function; the caller owns the lifetime.
 */
export async function attachEventBridge(client: QueryClient): Promise<() => void> {
  const unlisten = await subscribeEvent<SiteState>(IPC_EVENTS.siteState, (state) => {
    client.setQueryData(queryKeys.site.state(), state)
  })
  return () => {
    unlisten()
  }
}
