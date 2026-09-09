import { useRouterState } from '@tanstack/react-router'

import { ChevronLeftIcon } from '@/components/icons/chevron-left'
import { ChevronRightIcon } from '@/components/icons/chevron-right'
import { RefreshCwIcon } from '@/components/icons/refresh-cw'
import { SquarePenIcon } from '@/components/icons/square-pen'
import { WindowControls } from '@/components/shell/window-controls'
import { Button } from '@/components/ui/button'
import { Kbd } from '@/components/ui/kbd'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { useAnimatedIcon } from '@/lib/animated-icon'
import { IS_MACOS, MOD_KEY, SIDEBAR_RAIL_W, TITLEBAR_H, TITLEBAR_INSET_LEFT } from '@/lib/chrome'
import { usePrefs } from '@/lib/prefs'
import { useNavigateSite, useSiteAction, useSiteState } from '@/lib/query'
import type { SiteAction } from '@/lib/tauri/types'

/**
 * The island's own titlebar. The island owning its chrome — rather than a shared band across the
 * top of the window — is what makes the content column read as one object instead of a slab between
 * two strips. It carries the page's title, the browser verbs X has no buttons for, and the one
 * action people reach for most.
 */
export function PaneTitlebar() {
  const pathname = useRouterState({ select: (state) => state.location.pathname })
  const site = useSiteState()
  const go = useNavigateSite()
  const { sidebarMode } = usePrefs()
  const [composeRef, composeHover] = useAnimatedIcon()

  // With the sidebar collapsed to its rail, the macOS traffic lights reach
  // past it into this band; the leading controls step aside for them.
  const insetLeft = IS_MACOS && sidebarMode === 'rail' ? TITLEBAR_INSET_LEFT - SIDEBAR_RAIL_W : 12

  const onIsland = pathname === '/'
  // X's title is empty for a moment on every load; the mark stands in for it.
  const siteTitle = site.data?.title ?? ''
  const title = onIsland ? (siteTitle === '' ? 'X' : siteTitle) : 'Settings'

  return (
    <header
      data-tauri-drag-region
      className="drag-region flex shrink-0 items-center gap-2 border-b border-border/50 pr-3"
      style={{ height: TITLEBAR_H, paddingLeft: insetLeft }}
    >
      {onIsland ? (
        <div className="flex items-center gap-0.5">
          <ActionButton action="back" label="Back" shortcut="[" icon={ChevronLeftIcon} />
          <ActionButton action="forward" label="Forward" shortcut="]" icon={ChevronRightIcon} />
          <ActionButton action="reload" label="Reload" shortcut="R" icon={RefreshCwIcon} />
        </div>
      ) : null}
      <h1 className="truncate font-display text-sm font-semibold tracking-tight">{title}</h1>
      <div className="ml-auto flex items-center gap-1.5">
        <Button
          size="sm"
          onClick={() => {
            go.mutate('compose')
          }}
          {...composeHover}
        >
          <SquarePenIcon ref={composeRef} data-icon="inline-start" />
          New post
        </Button>
        <WindowControls className="-mr-1.5" />
      </div>
    </header>
  )
}

function ActionButton({
  action,
  label,
  shortcut,
  icon: Icon,
}: {
  action: SiteAction
  label: string
  shortcut: string
  icon: typeof ChevronLeftIcon
}) {
  const act = useSiteAction()
  const [iconRef, iconHover] = useAnimatedIcon()
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <Button
            size="icon-sm"
            variant="ghost"
            aria-label={label}
            onClick={() => {
              act.mutate(action)
            }}
            {...iconHover}
          >
            <Icon ref={iconRef} size={15} />
          </Button>
        }
      />
      <TooltipContent side="bottom">
        {label}
        <Kbd>
          {MOD_KEY}
          {shortcut}
        </Kbd>
      </TooltipContent>
    </Tooltip>
  )
}
