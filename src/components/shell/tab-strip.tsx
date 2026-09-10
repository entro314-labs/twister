import { ArcTabs } from '@entro314labs/react-arc-tabs'
import type { ArcTabItem } from '@entro314labs/react-arc-tabs'
import {
  IconBell,
  IconBookmark,
  IconBrandX,
  IconCompass,
  IconHome,
  IconMail,
  IconPencil,
  IconPlus,
  IconUser,
  IconX,
} from '@tabler/icons-react'
import { useNavigate, useRouterState } from '@tanstack/react-router'
import * as React from 'react'

import { MOD_KEY } from '@/lib/chrome'
import { useActivateTab, useCloseTab, useNewTab, useSiteState } from '@/lib/query'
import type { Section, TabState } from '@/lib/tauri/types'
import { useTip } from '@/lib/tooltip'
import { cn } from '@/lib/utils'

const SECTION_ICON: Record<Section, React.ElementType> = {
  home: IconHome,
  explore: IconCompass,
  notifications: IconBell,
  messages: IconMail,
  bookmarks: IconBookmark,
  profile: IconUser,
  compose: IconPencil,
  other: IconBrandX,
}

/**
 * The open tabs: each one an X page in its own webview. The strip fills the middle of the island's
 * titlebar and is a drag region like the rest of it — the library owns the strip's DOM, so the
 * attribute Tauri looks for is stamped onto its list elements after mount. Tabs themselves stay
 * clickable.
 */
export function TabStrip() {
  const site = useSiteState()
  const activate = useActivateTab()
  const close = useCloseTab()
  const open = useNewTab()
  const navigate = useNavigate()
  const pathname = useRouterState({ select: (state) => state.location.pathname })
  const newTip = useTip('New tab', `${MOD_KEY}T`, 'bottom')
  const bar = React.useRef<HTMLDivElement>(null)

  React.useEffect(() => {
    const root = bar.current
    if (!root) return
    for (const element of root.querySelectorAll<HTMLElement>(
      '.arc-tabs, .arc-tabs__list-scroll, .arc-tabs__list',
    )) {
      element.dataset.tauriDragRegion = ''
    }
  })

  // Read straight off the cached state so the memos key on one stable object.
  const state = site.data
  const tabs = React.useMemo(() => state?.tabs ?? [], [state])
  const active = state?.active ?? 0

  const items = React.useMemo<ArcTabItem[]>(
    () => tabs.map((tab) => ({ id: String(tab.id), label: tab.title || 'X', content: null })),
    [tabs],
  )
  const byId = React.useMemo(() => new Map(tabs.map((tab) => [String(tab.id), tab])), [tabs])

  const renderTabLabel = React.useCallback(
    (item: ArcTabItem) => {
      const tab = byId.get(item.id)
      if (!tab) return null
      return (
        <TabLabel
          tab={tab}
          closable={tabs.length > 1}
          active={tab.id === active}
          onClose={() => {
            close.mutate(tab.id)
          }}
        />
      )
    },
    [byId, tabs.length, close, active],
  )

  return (
    <div
      ref={bar}
      data-tauri-drag-region
      className="drag-region flex min-w-0 flex-1 items-center gap-1 self-stretch"
    >
      <div className="arc-tabs-theme flex min-w-0 items-center">
        <ArcTabs
          items={items}
          value={String(active)}
          onValueChange={(id) => {
            const parsed = Number(id)
            if (Number.isFinite(parsed) && parsed !== active) activate.mutate(parsed)
            // A tab picked from Settings or a tool goes back to the island.
            if (pathname !== '/' && !pathname.startsWith('/tools')) void navigate({ to: '/' })
          }}
          activationMode="manual"
          // A div trigger, so the close control can be a real nested button.
          tabElement="div"
          size="sm"
          fit="content"
          motionPreset="subtle"
          renderTabLabel={renderTabLabel}
          renderPanel={() => null}
          panelPadding={0}
          keepMounted
          ariaLabel="Open tabs"
        />
      </div>
      <button
        type="button"
        aria-label="New tab"
        {...newTip}
        onClick={() => {
          open.mutate(undefined)
        }}
        className="grid size-7 shrink-0 place-items-center rounded-md text-muted-foreground transition-colors hover:bg-muted/70 hover:text-foreground"
      >
        <IconPlus className="size-4" stroke={1.75} />
      </button>
    </div>
  )
}

function TabLabel({
  tab,
  closable,
  active,
  onClose,
}: {
  tab: TabState
  closable: boolean
  active: boolean
  onClose: () => void
}) {
  const Icon = SECTION_ICON[tab.section]
  const closeTip = useTip('Close tab', `${MOD_KEY}W`, 'bottom')
  return (
    <span
      className="group/tab inline-flex max-w-44 min-w-0 items-center gap-1.5"
      title={tab.url}
      onAuxClick={(event) => {
        // Middle-click closes, the way every browser's strip does.
        if (event.button === 1 && closable) {
          event.preventDefault()
          onClose()
        }
      }}
    >
      <span className="relative shrink-0">
        <Icon className={cn('size-3.5', tab.loading && 'animate-pulse')} stroke={1.75} />
        {tab.unread > 0 ? (
          <span className="absolute -top-1 -right-1 size-1.5 rounded-full bg-primary" />
        ) : null}
      </span>
      <span className={cn('truncate text-xs', active ? 'font-semibold' : 'font-medium')}>
        {tab.title || 'X'}
      </span>
      {closable ? (
        <button
          type="button"
          aria-label="Close tab"
          {...closeTip}
          onClick={(event) => {
            event.stopPropagation()
            onClose()
          }}
          className="-mr-1 grid size-4 shrink-0 place-items-center rounded-sm text-muted-foreground opacity-0 transition-opacity group-hover/tab:opacity-100 hover:bg-muted hover:text-foreground focus-visible:opacity-100"
        >
          <IconX className="size-3" stroke={2} />
        </button>
      ) : null}
    </span>
  )
}
