import type { QueryClient } from '@tanstack/react-query'

import { subscribeEvent } from '@/lib/tauri/client'
import { IPC_EVENTS } from '@/lib/tauri/ipc'
import type { Job, OpsState, SiteState, UpdateProgress } from '@/lib/tauri/types'

import { queryKeys } from './keys'

/**
 * Bridges Rust's pushes onto the query cache. The site state changes when x.com does — a
 * navigation, a new title, an unread count — so it is written straight into the cache rather than
 * refetched: the payload IS the answer. The running operation arrives the same way, as does an
 * update's download progress; the schedule only says it changed, and the list is refetched.
 *
 * Returns a detach function; the caller owns the lifetime.
 */
export async function attachEventBridge(client: QueryClient): Promise<() => void> {
  const detachers = await Promise.all([
    subscribeEvent<SiteState>(IPC_EVENTS.siteState, (state) => {
      client.setQueryData(queryKeys.site.state(), state)
    }),
    subscribeEvent<Job>(IPC_EVENTS.op, (job) => {
      const terminal = job.status !== 'running' && job.status !== 'queued'
      client.setQueryData<OpsState>(queryKeys.ops.state(), (previous) => ({
        running: terminal ? null : job,
        recent: terminal
          ? [job, ...(previous?.recent ?? []).filter((j) => j.id !== job.id)].slice(0, 20)
          : (previous?.recent ?? []),
      }))
      if (terminal) {
        // A finished scan or delete changed what the store holds.
        void client.invalidateQueries({ queryKey: queryKeys.store.root })
      }
    }),
    subscribeEvent<null>(IPC_EVENTS.schedule, () => {
      void client.invalidateQueries({ queryKey: queryKeys.schedule.root })
    }),
    subscribeEvent<UpdateProgress>(IPC_EVENTS.updateProgress, (progress) => {
      // Held in the cache rather than in the screen that started the download:
      // Settings and the status bar both read it, and neither can be the owner.
      client.setQueryData(queryKeys.update.progress(), progress)
    }),
  ])
  return () => {
    for (const detach of detachers) detach()
  }
}
