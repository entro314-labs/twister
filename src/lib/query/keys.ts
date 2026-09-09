/** Centralised query keys. Two domains: the settings file, and the site state Rust pushes. */
export const queryKeys = {
  settings: {
    root: ['settings'] as const,
    current: () => [...queryKeys.settings.root, 'current'] as const,
  },
  site: {
    root: ['site'] as const,
    state: () => [...queryKeys.site.root, 'state'] as const,
  },
} as const
