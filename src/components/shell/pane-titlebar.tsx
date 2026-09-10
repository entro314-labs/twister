import { ChevronLeftIcon } from '@/components/icons/chevron-left'
import { ChevronRightIcon } from '@/components/icons/chevron-right'
import { RefreshCwIcon } from '@/components/icons/refresh-cw'
import { SquarePenIcon } from '@/components/icons/square-pen'
import { TabStrip } from '@/components/shell/tab-strip'
import { WindowControls } from '@/components/shell/window-controls'
import { Button } from '@/components/ui/button'
import { useAnimatedIcon } from '@/lib/animated-icon'
import { MOD_KEY, TITLEBAR_H } from '@/lib/chrome'
import { useNavigateSite, useSiteAction } from '@/lib/query'
import type { SiteAction } from '@/lib/tauri/types'
import { useTip, withHandlers } from '@/lib/tooltip'

/**
 * The island's own chrome, in one band: the browser verbs X has no buttons for, the open tabs —
 * each one carrying its page's title, so the band needs no title of its own — the one action people
 * reach for most, and the window controls. The whole band is a drag region. The island owning its
 * chrome — rather than a shared band across the top of the window — is what makes the content
 * column read as one object instead of a slab between two strips.
 */
export function PaneTitlebar() {
  const go = useNavigateSite()
  const [composeRef, composeHover] = useAnimatedIcon()
  const composeTip = useTip('New post', `${MOD_KEY}N`, 'bottom')

  return (
    <header
      data-tauri-drag-region
      className="drag-region flex shrink-0 items-center gap-3 px-3"
      style={{ height: TITLEBAR_H }}
    >
      <div className="flex shrink-0 items-center gap-0.5">
        <ActionButton action="back" label="Back" shortcut="[" icon={ChevronLeftIcon} />
        <ActionButton action="forward" label="Forward" shortcut="]" icon={ChevronRightIcon} />
        <ActionButton action="reload" label="Reload" shortcut="R" icon={RefreshCwIcon} />
      </div>
      <TabStrip />
      <div className="flex shrink-0 items-center gap-1.5">
        <Button
          size="icon-sm"
          aria-label="New post"
          onClick={() => {
            go.mutate('compose')
          }}
          {...withHandlers(composeTip, composeHover)}
        >
          <SquarePenIcon ref={composeRef} size={15} />
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
  const tip = useTip(label, `${MOD_KEY}${shortcut}`, 'bottom')
  return (
    <Button
      size="icon-sm"
      variant="ghost"
      aria-label={label}
      onClick={() => {
        act.mutate(action)
      }}
      {...withHandlers(tip, iconHover)}
    >
      <Icon ref={iconRef} size={15} />
    </Button>
  )
}
