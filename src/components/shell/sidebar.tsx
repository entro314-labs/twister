import {
  IconBell,
  IconBookmark,
  IconCompass,
  IconHome,
  IconMail,
  IconNotes,
  IconPencil,
  IconUser,
  IconUsers,
} from '@tabler/icons-react'
import { Link, useNavigate, useRouterState } from '@tanstack/react-router'
import * as React from 'react'

import { PanelLeftCloseIcon } from '@/components/icons/panel-left-close'
import { PanelLeftOpenIcon } from '@/components/icons/panel-left-open'
import { SettingsIcon } from '@/components/icons/settings'
import { useAnimatedIcon } from '@/lib/animated-icon'
import {
  APP_NAME,
  IS_MACOS,
  MOD_KEY,
  SIDEBAR_MAX_W,
  SIDEBAR_MIN_W,
  SIDEBAR_RAIL_W,
  TITLEBAR_H,
  TITLEBAR_INSET_LEFT,
  TITLEBAR_INSET_LEFT_FULLSCREEN,
} from '@/lib/chrome'
import { useIsFullscreen } from '@/lib/fullscreen'
import { usePrefs } from '@/lib/prefs'
import { useActiveTab, useNavigateSite, useSiteState } from '@/lib/query'
import type { Destination, Section } from '@/lib/tauri/types'
import { useTip, withHandlers } from '@/lib/tooltip'
import { cn } from '@/lib/utils'

interface NavItem {
  destination: Destination
  section: Section
  label: string
  shortcut: string
  icon: React.ElementType
}

// The same six destinations the View menu carries, in the same order, with
// the same shortcuts — the sidebar is the menu you can see.
const NAV: NavItem[] = [
  { destination: 'home', section: 'home', label: 'Home', shortcut: '1', icon: IconHome },
  {
    destination: 'explore',
    section: 'explore',
    label: 'Explore',
    shortcut: '2',
    icon: IconCompass,
  },
  {
    destination: 'notifications',
    section: 'notifications',
    label: 'Notifications',
    shortcut: '3',
    icon: IconBell,
  },
  {
    destination: 'messages',
    section: 'messages',
    label: 'Messages',
    shortcut: '4',
    icon: IconMail,
  },
  {
    destination: 'bookmarks',
    section: 'bookmarks',
    label: 'Bookmarks',
    shortcut: '5',
    icon: IconBookmark,
  },
  { destination: 'profile', section: 'profile', label: 'Profile', shortcut: '6', icon: IconUser },
]

interface ToolItem {
  to: '/tools/people' | '/tools/posts' | '/tools/compose'
  label: string
  shortcut: string
  icon: React.ElementType
}

// The tools open a panel beside the island rather than moving the site.
const TOOLS: ToolItem[] = [
  { to: '/tools/people', label: 'People', shortcut: '⇧P', icon: IconUsers },
  { to: '/tools/posts', label: 'Posts', shortcut: '⇧O', icon: IconNotes },
  { to: '/tools/compose', label: 'Write', shortcut: '⇧N', icon: IconPencil },
]

/** Sidebar glyphs: one step larger than the text so a rail of icons reads at a glance. */
const ICON_SIZE = 18

/**
 * The navigation column — and, with the system frame gone, the window's leading chrome. It runs the
 * full window height, and its header band is sized (TITLEBAR_H) to the macOS title bar AppKit draws
 * the traffic lights in, so the lights sit centred in it; there is no shared horizontal titlebar,
 * and the island carries its own (see PaneTitlebar).
 *
 * Two states. The outer shell animates its width while the inner wrapper keeps a fixed one, so
 * content slides out of the clip instead of squashing mid-animation.
 */
export function Sidebar() {
  const { sidebarMode } = usePrefs()

  return (
    <aside
      data-mode={sidebarMode}
      className={cn(
        'relative flex h-full shrink-0 flex-col overflow-hidden',
        // Deliberately no background: the sidebar sits directly on the desk,
        // which is the one translucent surface and therefore the one the OS
        // effect can show through.
        'text-sidebar-foreground',
        'transition-[width] duration-300 ease-[cubic-bezier(0.22,1,0.36,1)]',
      )}
      style={{
        width:
          sidebarMode === 'full'
            ? 'var(--pane-sidebar-w, var(--pane-sidebar-default))'
            : SIDEBAR_RAIL_W,
      }}
      aria-label="Primary navigation"
    >
      <div
        aria-hidden
        className="pointer-events-none absolute inset-x-0 top-0 h-40 bg-gradient-to-b from-primary/[0.07] via-primary/[0.02] to-transparent"
      />

      {sidebarMode === 'rail' ? <RailContent /> : <FullContent />}
      {sidebarMode === 'full' ? <ResizeSeam /> : null}
    </aside>
  )
}

function FullContent() {
  // The traffic lights ride in this band, so the wordmark starts after them — except in
  // fullscreen, where the OS takes them away and the reserved space would just be a hole.
  const isFullscreen = useIsFullscreen()
  const insetLeft = isFullscreen ? TITLEBAR_INSET_LEFT_FULLSCREEN : TITLEBAR_INSET_LEFT

  return (
    <div
      className="flex h-full flex-col"
      style={{ width: 'var(--pane-sidebar-w, var(--pane-sidebar-default))' }}
    >
      <header
        data-tauri-drag-region
        className="drag-region flex shrink-0 items-center"
        style={{
          height: TITLEBAR_H,
          paddingLeft: IS_MACOS ? insetLeft : 14,
        }}
      >
        <span className="pointer-events-none truncate font-display text-sm font-semibold tracking-tight">
          {APP_NAME}
        </span>
        <LayoutButton />
      </header>

      <nav className="flex min-h-0 flex-1 flex-col gap-0.5 overflow-y-auto px-2 py-1">
        {NAV.map((item) => (
          <NavRow key={item.destination} item={item} />
        ))}
        <Divider />
        {TOOLS.map((item) => (
          <ToolRow key={item.to} item={item} />
        ))}
      </nav>

      <div className="flex flex-col gap-0.5 px-2 pb-2">
        <SettingsRow />
      </div>
    </div>
  )
}

function RailContent() {
  return (
    <div className="flex h-full flex-col" style={{ width: SIDEBAR_RAIL_W }}>
      {/* On macOS the traffic lights own this band — the rail is wide enough
          (SIDEBAR_RAIL_W) that all three fit inside its column — so the toggle
          takes the next row, on the same axis as every icon under it. */}
      <header
        data-tauri-drag-region
        className="drag-region flex shrink-0 items-center justify-center"
        style={{ height: TITLEBAR_H }}
      >
        {IS_MACOS ? null : <LayoutButton rail />}
      </header>
      {IS_MACOS ? (
        <div className="flex shrink-0 justify-center pb-1">
          <LayoutButton rail />
        </div>
      ) : null}
      <nav className="flex min-h-0 flex-1 flex-col items-center gap-1 overflow-y-auto py-1">
        {NAV.map((item) => (
          <NavRow key={item.destination} item={item} rail />
        ))}
        <Divider rail />
        {TOOLS.map((item) => (
          <ToolRow key={item.to} item={item} rail />
        ))}
      </nav>
      <div className="flex flex-col items-center gap-1 pb-2">
        <SettingsRow rail />
      </div>
    </div>
  )
}

const rowClass = (active: boolean, rail: boolean, disabled = false) =>
  cn(
    'group relative flex items-center rounded-md text-sm transition-colors duration-[var(--motion-micro)]',
    rail ? 'size-9 justify-center' : 'h-8 gap-2.5 px-2.5',
    active
      ? 'bg-sidebar-accent font-medium text-sidebar-accent-foreground'
      : 'text-muted-foreground hover:bg-sidebar-accent/60 hover:text-foreground',
    disabled && 'opacity-50 hover:bg-transparent hover:text-muted-foreground',
  )

function Divider({ rail = false }: { rail?: boolean }) {
  return <div aria-hidden className={cn('my-1.5 h-px bg-border/60', rail ? 'w-5' : 'mx-2.5')} />
}

/** A tool panel, beside the island. Clicking the open one closes it. */
function ToolRow({ item, rail = false }: { item: ToolItem; rail?: boolean }) {
  const pathname = useRouterState({ select: (state) => state.location.pathname })
  const navigate = useNavigate()
  const active = pathname === item.to
  const Icon = item.icon
  const tip = useTip(item.label, `${MOD_KEY}${item.shortcut}`)

  return (
    <button
      type="button"
      aria-current={active ? 'page' : undefined}
      {...tip}
      onClick={() => {
        void navigate({ to: active ? '/' : item.to })
      }}
      className={rowClass(active, rail)}
    >
      <ActiveMarker active={active} />
      <Icon size={ICON_SIZE} stroke={1.75} className="shrink-0" />
      {rail ? null : <span className="truncate">{item.label}</span>}
    </button>
  )
}

function ActiveMarker({ active }: { active: boolean }) {
  return (
    // A short bar on the leading edge rather than a full border, so a
    // selected row reads as attached to the column.
    <span
      aria-hidden
      className={cn(
        'absolute inset-y-1.5 left-0 w-0.5 rounded-full bg-primary transition-opacity',
        active ? 'opacity-100' : 'opacity-0',
      )}
    />
  )
}

/**
 * One destination on X. A row is a button rather than a link because it moves the SITE, not this
 * page — and if this page is on Settings, it moves back to the island as well.
 */
function NavRow({ item, rail = false }: { item: NavItem; rail?: boolean }) {
  const pathname = useRouterState({ select: (state) => state.location.pathname })
  const navigate = useNavigate()
  const site = useSiteState()
  const tab = useActiveTab()
  const go = useNavigateSite()

  const onIsland = pathname === '/' || pathname.startsWith('/tools')
  const active = onIsland && tab?.section === item.section
  // Profile needs the handle, which the bridge only learns once X has drawn
  // its own navigation — a row that cannot go anywhere yet says so.
  const disabled = item.destination === 'profile' && !site.data?.handle
  const unread = item.section === 'notifications' ? (tab?.unread ?? 0) : 0
  const Icon = item.icon
  const tip = useTip(item.label, `${MOD_KEY}${item.shortcut}`)

  return (
    <button
      type="button"
      aria-current={active ? 'page' : undefined}
      disabled={disabled}
      {...tip}
      onClick={() => {
        go.mutate(item.destination)
        if (!onIsland) void navigate({ to: '/' })
      }}
      className={rowClass(active, rail, disabled)}
    >
      <ActiveMarker active={active} />
      <span className="relative shrink-0">
        <Icon size={ICON_SIZE} stroke={1.75} />
        {rail && unread > 0 ? (
          <span className="absolute -top-1 -right-1 size-2 rounded-full bg-primary ring-2 ring-[var(--sidebar)]" />
        ) : null}
      </span>
      {rail ? null : (
        <>
          <span className="truncate">{item.label}</span>
          {item.destination === 'profile' && site.data?.handle ? (
            <span className="truncate text-xs text-muted-foreground/80">@{site.data.handle}</span>
          ) : null}
          {unread > 0 ? (
            <span className="ml-auto rounded-4xl bg-primary px-1.5 text-[10px] font-semibold text-primary-foreground tabular-nums">
              {unread > 99 ? '99+' : unread}
            </span>
          ) : null}
        </>
      )}
    </button>
  )
}

function SettingsRow({ rail = false }: { rail?: boolean }) {
  const pathname = useRouterState({ select: (state) => state.location.pathname })
  const active = pathname.startsWith('/settings')
  const [iconRef, iconHover] = useAnimatedIcon()
  const tip = useTip('Settings', `${MOD_KEY},`)

  return (
    <Link
      to="/settings"
      aria-current={active ? 'page' : undefined}
      {...withHandlers(tip, iconHover)}
      className={rowClass(active, rail)}
    >
      <ActiveMarker active={active} />
      <SettingsIcon ref={iconRef} size={ICON_SIZE} className="shrink-0" />
      {rail ? null : <span className="truncate">Settings</span>}
    </Link>
  )
}

function LayoutButton({ rail = false }: { rail?: boolean }) {
  const { sidebarMode, setSidebarMode } = usePrefs()
  const Icon = sidebarMode === 'full' ? PanelLeftCloseIcon : PanelLeftOpenIcon
  const [iconRef, iconHover] = useAnimatedIcon()
  const label = sidebarMode === 'full' ? 'Collapse the sidebar' : 'Expand the sidebar'
  const tip = useTip(label, `${MOD_KEY}\\`)
  return (
    <button
      type="button"
      aria-label={label}
      onClick={() => {
        setSidebarMode(sidebarMode === 'full' ? 'rail' : 'full')
      }}
      {...withHandlers(tip, iconHover)}
      className={cn(
        'grid shrink-0 place-items-center rounded-md text-muted-foreground transition-colors hover:bg-sidebar-accent hover:text-foreground',
        // In the rail the toggle is one of the icons and sits on their axis;
        // in the full sidebar it is the header's trailing control.
        rail ? 'size-9' : 'mr-2 ml-auto size-7',
      )}
    >
      <Icon ref={iconRef} size={ICON_SIZE} />
    </button>
  )
}

/**
 * Drag-to-resize at the sidebar's trailing edge.
 *
 * The width is written straight to the custom property during the drag and only committed to state
 * on release: re-rendering the whole shell on every pointermove is what makes a resize handle feel
 * like it is lagging behind the cursor. The island's ResizeObserver follows the property, so the
 * site webview tracks the drag frame by frame.
 */
function ResizeSeam() {
  const { sidebarWidth, setSidebarWidth } = usePrefs()

  const onPointerDown = React.useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      event.preventDefault()
      const startX = event.clientX
      const startWidth = sidebarWidth
      let latest = startWidth

      const onMove = (move: PointerEvent) => {
        latest = Math.min(
          SIDEBAR_MAX_W,
          Math.max(SIDEBAR_MIN_W, startWidth + move.clientX - startX),
        )
        document.documentElement.style.setProperty('--pane-sidebar-w', `${latest}px`)
      }
      const onUp = () => {
        window.removeEventListener('pointermove', onMove)
        window.removeEventListener('pointerup', onUp)
        document.body.style.cursor = ''
        setSidebarWidth(latest)
      }

      document.body.style.cursor = 'col-resize'
      window.addEventListener('pointermove', onMove)
      window.addEventListener('pointerup', onUp)
    },
    [sidebarWidth, setSidebarWidth],
  )

  return (
    <div
      role="separator"
      aria-orientation="vertical"
      aria-label="Resize the sidebar"
      onPointerDown={onPointerDown}
      // Four pixels wide but reaching further with a pseudo-element: a 1px seam
      // is honest visually and miserable to hit.
      className="absolute inset-y-0 right-0 z-10 w-1 cursor-col-resize after:absolute after:inset-y-0 after:-left-1.5 after:w-4 hover:bg-primary/30"
    />
  )
}
