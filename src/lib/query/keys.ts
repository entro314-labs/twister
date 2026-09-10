import type { PersonFilter, PostFilter } from '@/lib/tauri/types'

/** Centralised query keys: the settings file, the site state Rust pushes, and the store. */
export const queryKeys = {
  settings: {
    root: ['settings'] as const,
    current: () => [...queryKeys.settings.root, 'current'] as const,
  },
  site: {
    root: ['site'] as const,
    state: () => [...queryKeys.site.root, 'state'] as const,
  },
  userland: {
    root: ['userland'] as const,
    list: () => [...queryKeys.userland.root, 'list'] as const,
  },
  store: {
    root: ['store'] as const,
    counts: () => [...queryKeys.store.root, 'counts'] as const,
    people: (filter: PersonFilter) => [...queryKeys.store.root, 'people', filter] as const,
    posts: (filter: PostFilter) => [...queryKeys.store.root, 'posts', filter] as const,
  },
  ops: {
    root: ['ops'] as const,
    state: () => [...queryKeys.ops.root, 'state'] as const,
  },
  schedule: {
    root: ['schedule'] as const,
    list: () => [...queryKeys.schedule.root, 'list'] as const,
  },
  update: {
    root: ['update'] as const,
    state: () => [...queryKeys.update.root, 'state'] as const,
    check: () => [...queryKeys.update.root, 'check'] as const,
    /** Live download bytes, written by the event bridge; absent when idle. */
    progress: () => [...queryKeys.update.root, 'progress'] as const,
  },
} as const
