import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { invokeCommand } from '@/lib/tauri/client'
import { IPC_COMMANDS } from '@/lib/tauri/ipc'
import type { Destination, Settings, SiteAction, SiteState } from '@/lib/tauri/types'

import { queryKeys } from './keys'

/**
 * The resolved value of a command that answers with nothing. Rust's `()` serializes as JSON `null`,
 * so this is `null` and not `undefined`.
 */
type Nothing = null

// ─── Reads ──────────────────────────────────────────────────────────────────

export function useSettings() {
  return useQuery({
    queryKey: queryKeys.settings.current(),
    queryFn: async () => invokeCommand<Settings>(IPC_COMMANDS.getSettings),
  })
}

/**
 * Fetched once; every change after that arrives by push (see `events.ts`), so the cache is never
 * stale and never refetched.
 */
export function useSiteState() {
  return useQuery({
    queryKey: queryKeys.site.state(),
    queryFn: async () => invokeCommand<SiteState>(IPC_COMMANDS.getSiteState),
    staleTime: Number.POSITIVE_INFINITY,
  })
}

// ─── Writes ─────────────────────────────────────────────────────────────────

/**
 * Rust answers with the settings it stored, which is what the cache should hold — not what the
 * renderer asked for, in case validation trimmed anything.
 */
export function useUpdateSettings() {
  const client = useQueryClient()
  return useMutation({
    mutationFn: async (settings: Settings) =>
      invokeCommand<Settings>(IPC_COMMANDS.updateSettings, { settings }),
    onSuccess: (stored) => {
      client.setQueryData(queryKeys.settings.current(), stored)
    },
  })
}

export function useNavigateSite() {
  return useMutation({
    mutationFn: async (destination: Destination) =>
      invokeCommand<Nothing>(IPC_COMMANDS.navigateSite, { destination }),
  })
}

export function useSiteAction() {
  return useMutation({
    mutationFn: async (action: SiteAction) =>
      invokeCommand<Nothing>(IPC_COMMANDS.siteAction, { action }),
  })
}

export function useSignOut() {
  return useMutation({
    mutationFn: async () => invokeCommand<Nothing>(IPC_COMMANDS.signOut),
  })
}
