import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { invokeCommand } from '@/lib/tauri/client'
import { IPC_COMMANDS } from '@/lib/tauri/ipc'
import type {
  Destination,
  ExportFormat,
  Job,
  OpKind,
  OpsState,
  Person,
  PersonFilter,
  Post,
  PostFilter,
  PreparedPost,
  ScheduledPost,
  Settings,
  SiteAction,
  SiteState,
  StoreCounts,
  TabState,
  UpdateMeta,
  UpdateProgress,
  UpdateState,
  UserAssets,
} from '@/lib/tauri/types'

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

/** The tab in front, or `undefined` before the first one exists. */
export function useActiveTab(): TabState | undefined {
  const site = useSiteState()
  return site.data?.tabs.find((tab) => tab.id === site.data?.active)
}

/** What is in the scripts and styles folder right now — not what is running. */
export function useUserAssets() {
  return useQuery({
    queryKey: queryKeys.userland.list(),
    queryFn: async () => invokeCommand<UserAssets>(IPC_COMMANDS.listUserAssets),
    // The folder changes behind the app's back, so the list is re-read each
    // time the settings screen shows it.
    staleTime: 0,
  })
}

export function useStoreCounts() {
  return useQuery({
    queryKey: queryKeys.store.counts(),
    queryFn: async () => invokeCommand<StoreCounts>(IPC_COMMANDS.getStoreCounts),
    staleTime: 5000,
  })
}

export function usePeople(filter: PersonFilter) {
  return useQuery({
    queryKey: queryKeys.store.people(filter),
    queryFn: async () => invokeCommand<Person[]>(IPC_COMMANDS.listPeople, { filter }),
    placeholderData: (previous) => previous,
  })
}

export function usePosts(filter: PostFilter) {
  return useQuery({
    queryKey: queryKeys.store.posts(filter),
    queryFn: async () => invokeCommand<Post[]>(IPC_COMMANDS.listPosts, { filter }),
    placeholderData: (previous) => previous,
  })
}

/** Fetched once; the running job is pushed from then on. */
export function useOps() {
  return useQuery({
    queryKey: queryKeys.ops.state(),
    queryFn: async () => invokeCommand<OpsState>(IPC_COMMANDS.getOps),
    staleTime: Number.POSITIVE_INFINITY,
  })
}

export function useScheduledPosts() {
  return useQuery({
    queryKey: queryKeys.schedule.list(),
    queryFn: async () => invokeCommand<ScheduledPost[]>(IPC_COMMANDS.listScheduledPosts),
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

export function useOpenUserAssetsDir() {
  return useMutation({
    mutationFn: async () => invokeCommand<Nothing>(IPC_COMMANDS.openUserAssetsDir),
  })
}

/** Rebuilds every tab so folder changes start running; the pages reload. */
export function useReloadSite() {
  const client = useQueryClient()
  return useMutation({
    mutationFn: async () => invokeCommand<Nothing>(IPC_COMMANDS.reloadSite),
    onSuccess: () => {
      void client.invalidateQueries({ queryKey: queryKeys.userland.root })
    },
  })
}

export function useNewTab() {
  return useMutation({
    mutationFn: async (url?: string) =>
      invokeCommand<number>(IPC_COMMANDS.newTab, { url: url ?? null }),
  })
}

export function useCloseTab() {
  return useMutation({
    mutationFn: async (id: number) => invokeCommand<Nothing>(IPC_COMMANDS.closeTab, { id }),
  })
}

export function useActivateTab() {
  return useMutation({
    mutationFn: async (id: number) => invokeCommand<Nothing>(IPC_COMMANDS.activateTab, { id }),
  })
}

/** Resolves with the path written, or `null` when the save dialog was dismissed. */
export function useExportPeople() {
  return useMutation({
    mutationFn: async (input: { filter: PersonFilter; format: ExportFormat }) =>
      invokeCommand<string | null>(IPC_COMMANDS.exportPeople, input),
  })
}

export function useExportPosts() {
  return useMutation({
    mutationFn: async (input: { filter: PostFilter; format: ExportFormat }) =>
      invokeCommand<string | null>(IPC_COMMANDS.exportPosts, input),
  })
}

export function useClearCaptured() {
  const client = useQueryClient()
  return useMutation({
    mutationFn: async () => invokeCommand<Nothing>(IPC_COMMANDS.clearCaptured),
    onSuccess: () => {
      void client.invalidateQueries({ queryKey: queryKeys.store.root })
    },
  })
}

export function useStartOp() {
  const client = useQueryClient()
  return useMutation({
    mutationFn: async (input: { kind: OpKind; params: Record<string, unknown>; dryRun: boolean }) =>
      invokeCommand<Job>(IPC_COMMANDS.startOp, input),
    onSuccess: (job) => {
      client.setQueryData<OpsState>(queryKeys.ops.state(), (previous) => ({
        running: job,
        recent: previous?.recent ?? [],
      }))
    },
  })
}

export function useCancelOp() {
  return useMutation({
    mutationFn: async () => invokeCommand<Nothing>(IPC_COMMANDS.cancelOp),
  })
}

export function usePreparePost(markdown: string) {
  return useQuery({
    queryKey: ['compose', 'prepare', markdown] as const,
    queryFn: async () => invokeCommand<PreparedPost>(IPC_COMMANDS.preparePost, { markdown }),
    placeholderData: (previous) => previous,
    staleTime: Number.POSITIVE_INFINITY,
  })
}

export function usePostNow() {
  const client = useQueryClient()
  return useMutation({
    mutationFn: async (markdown: string) => invokeCommand<Job>(IPC_COMMANDS.postNow, { markdown }),
    onSuccess: (job) => {
      client.setQueryData<OpsState>(queryKeys.ops.state(), (previous) => ({
        running: job,
        recent: previous?.recent ?? [],
      }))
    },
  })
}

export function useSchedulePost() {
  const client = useQueryClient()
  return useMutation({
    mutationFn: async (input: { markdown: string; scheduledAt: string }) =>
      invokeCommand<ScheduledPost>(IPC_COMMANDS.schedulePost, input),
    onSuccess: () => {
      void client.invalidateQueries({ queryKey: queryKeys.schedule.root })
    },
  })
}

export function useDeleteScheduledPost() {
  const client = useQueryClient()
  return useMutation({
    mutationFn: async (id: number) =>
      invokeCommand<Nothing>(IPC_COMMANDS.deleteScheduledPost, { id }),
    onSuccess: () => {
      void client.invalidateQueries({ queryKey: queryKeys.schedule.root })
    },
  })
}

export function useOpenDownloadsDir() {
  return useMutation({
    mutationFn: async () => invokeCommand<Nothing>(IPC_COMMANDS.openDownloadsDir),
  })
}

// ─── Updates ────────────────────────────────────────────────────────────────

/** Which version is running, whether it can self-update, and whether one is already staged. */
export function useUpdateState() {
  return useQuery({
    queryKey: queryKeys.update.state(),
    queryFn: async () => invokeCommand<UpdateState>(IPC_COMMANDS.updateState),
    staleTime: Number.POSITIVE_INFINITY,
  })
}

/**
 * The last check's answer — a newer build, or `null` for "nothing newer". Never runs on mount: a
 * check reaches the network, so it only ever happens because something asked. Settings, the menu
 * item and the launch check all `refetch()` this one query, so all three read the same answer and
 * the same failure.
 */
export function useUpdateCheck() {
  return useQuery({
    queryKey: queryKeys.update.check(),
    queryFn: async () => invokeCommand<UpdateMeta | null>(IPC_COMMANDS.checkForUpdate),
    enabled: false,
    staleTime: Number.POSITIVE_INFINITY,
    gcTime: Number.POSITIVE_INFINITY,
    retry: false,
  })
}

/** Live download bytes, written by the event bridge; undefined when nothing is downloading. */
export function useUpdateProgress() {
  return useQuery({
    queryKey: queryKeys.update.progress(),
    queryFn: (): UpdateProgress | null => null,
    enabled: false,
    staleTime: Number.POSITIVE_INFINITY,
    gcTime: Number.POSITIVE_INFINITY,
  }).data
}

/** Download and verify the update, and hold it for the next quit. Nothing is replaced until then. */
export function useStageUpdate() {
  const client = useQueryClient()
  return useMutation({
    mutationFn: async () => invokeCommand<UpdateMeta>(IPC_COMMANDS.stageUpdate),
    onMutate: () => {
      // A previous download's last frame must not be the first thing this one shows.
      client.setQueryData(queryKeys.update.progress(), null)
    },
    onSettled: () => {
      void client.invalidateQueries({ queryKey: queryKeys.update.state() })
    },
  })
}

/** Install the staged bundle now and relaunch into it. On success nothing here runs again. */
export function useRestartAndInstall() {
  const client = useQueryClient()
  return useMutation({
    mutationFn: async () => invokeCommand<Nothing>(IPC_COMMANDS.restartAndInstall),
    onError: () => {
      // A failed install leaves the bundle staged for a retry — re-read that.
      void client.invalidateQueries({ queryKey: queryKeys.update.state() })
    },
  })
}
