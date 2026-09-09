import * as React from 'react'

import { useSiteState } from '@/lib/query'
import { subscribeEvent } from '@/lib/tauri/client'
import { IPC_EVENTS } from '@/lib/tauri/ipc'
import type { Notice, Section } from '@/lib/tauri/types'
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

/**
 * The island's footer: where on X the page is, whether it is still loading, and the one-line
 * notices Rust sends — "opened example.com in your browser" is the whole difference between a link
 * that went somewhere and one that silently did nothing.
 */
export function StatusBar() {
  const site = useSiteState()
  const notice = useNotice()

  const path = pathOf(site.data?.url)

  return (
    <footer className="flex h-6 shrink-0 items-center gap-3 border-t border-border/50 px-3 text-[11px] text-muted-foreground">
      <span className="flex min-w-0 items-center gap-1.5">
        <span
          aria-hidden
          className={cn(
            'size-1.5 shrink-0 rounded-full',
            site.data?.loading ? 'animate-[var(--animate-carrier)] bg-primary' : 'bg-success',
          )}
        />
        <span className="shrink-0">{SECTION_LABEL[site.data?.section ?? 'other']}</span>
        {path ? <span className="truncate font-mono text-muted-foreground/70">{path}</span> : null}
      </span>

      {site.data && site.data.unread > 0 ? (
        <span className="shrink-0 tabular-nums">{site.data.unread} unread</span>
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
        }, 5000)
      })
    })()
    return () => {
      detach?.()
      if (timer) clearTimeout(timer)
    }
  }, [])

  return notice
}
