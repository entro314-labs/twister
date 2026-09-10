import * as React from 'react'

import { useActiveTab, useCancelOp, useOps } from '@/lib/query'
import { subscribeEvent } from '@/lib/tauri/client'
import { IPC_EVENTS } from '@/lib/tauri/ipc'
import type { Notice, OpKind, Section } from '@/lib/tauri/types'
import { cn } from '@/lib/utils'

const SECTION_LABEL: Record<Section, string> = {
  home: 'Home',
  explore: 'Explore',
  notifications: 'Notifications',
  messages: 'Messages',
  bookmarks: 'Bookmarks',
  profile: 'Profile',
  compose: 'New post',
  other: 'X',
}

const OP_LABEL: Record<OpKind, string> = {
  scan: 'Scanning',
  follow: 'Following',
  unfollow: 'Unfollowing',
  delete: 'Deleting',
  compose: 'Posting',
}

/**
 * The island's footer: where on X the front tab is, whether it is still loading, the operation
 * running in it, and the one-line notices Rust sends — "opened example.com in your browser" is the
 * whole difference between a link that went somewhere and one that silently did nothing.
 */
export function StatusBar() {
  const tab = useActiveTab()
  const ops = useOps()
  const cancel = useCancelOp()
  const notice = useNotice()

  const path = pathOf(tab?.url)
  const running = ops.data?.running ?? null

  return (
    <footer className="flex h-6 shrink-0 items-center gap-3 border-t border-border/50 px-3 text-[11px] text-muted-foreground">
      <span className="flex min-w-0 items-center gap-1.5">
        <span
          aria-hidden
          className={cn(
            'size-1.5 shrink-0 rounded-full',
            tab?.loading ? 'animate-[var(--animate-carrier)] bg-primary' : 'bg-success',
          )}
        />
        <span className="shrink-0">{SECTION_LABEL[tab?.section ?? 'other']}</span>
        {path ? <span className="truncate font-mono text-muted-foreground/70">{path}</span> : null}
      </span>

      {tab && tab.unread > 0 ? (
        <span className="shrink-0 tabular-nums">{tab.unread} unread</span>
      ) : null}

      {running ? (
        <span className="flex min-w-0 items-center gap-2 text-foreground/80">
          <span className="size-1.5 shrink-0 animate-[var(--animate-carrier)] rounded-full bg-warning" />
          <span className="truncate">
            {OP_LABEL[running.kind]}
            {running.dryRun ? ' (dry run)' : ''}
            {running.total > 0 ? ` ${running.done}/${running.total}` : ''}
            {running.message ? ` · ${running.message}` : ''}
          </span>
          <button
            type="button"
            onClick={() => {
              cancel.mutate()
            }}
            className="shrink-0 rounded-sm px-1 text-destructive hover:bg-destructive/10"
          >
            Stop
          </button>
        </span>
      ) : null}

      {notice ? <span className="ml-auto truncate text-foreground/80">{notice}</span> : null}
    </footer>
  )
}

function pathOf(href: string | undefined): string {
  if (!href) return ''
  try {
    const url = new URL(href)
    return url.pathname === '/' ? '' : url.pathname
  } catch {
    return ''
  }
}

/** The latest notice, for a few seconds. */
function useNotice(): string | null {
  const [notice, setNotice] = React.useState<string | null>(null)

  React.useEffect(() => {
    let detach: (() => void) | null = null
    let timer: ReturnType<typeof setTimeout> | null = null
    void (async () => {
      detach = await subscribeEvent<Notice>(IPC_EVENTS.notice, ({ message }) => {
        setNotice(message)
        if (timer) clearTimeout(timer)
        timer = setTimeout(() => {
          setNotice(null)
        }, 6000)
      })
    })()
    return () => {
      detach?.()
      if (timer) clearTimeout(timer)
    }
  }, [])

  return notice
}
