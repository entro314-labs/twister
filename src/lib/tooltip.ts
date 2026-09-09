import * as React from 'react'

import { invokeCommand } from '@/lib/tauri/client'
import { IPC_COMMANDS } from '@/lib/tauri/ipc'
import type { TooltipSide } from '@/lib/tauri/types'
import { useTheme } from '@/lib/theme'

const OPEN_DELAY = 350

/**
 * A tooltip drawn by Rust in its own window, so it can float over the site webview — which covers
 * the island and would hide anything this page drew there. Spread the handlers on the control; the
 * anchor is measured on the way in, so a row that moved (a sidebar resize) is still placed
 * correctly.
 */
export function useTip(label: string, shortcut: string | null = null, side: TooltipSide = 'right') {
  const { resolvedTheme } = useTheme()
  const timer = React.useRef<ReturnType<typeof setTimeout> | null>(null)
  const shown = React.useRef(false)

  const hide = React.useCallback(() => {
    if (timer.current) {
      clearTimeout(timer.current)
      timer.current = null
    }
    if (!shown.current) return
    shown.current = false
    void invokeCommand(IPC_COMMANDS.hideTooltip).catch(() => {})
  }, [])

  const show = React.useCallback(
    (element: HTMLElement) => {
      if (timer.current) clearTimeout(timer.current)
      timer.current = setTimeout(() => {
        timer.current = null
        const rect = element.getBoundingClientRect()
        shown.current = true
        void invokeCommand(IPC_COMMANDS.showTooltip, {
          anchor: { x: rect.left, y: rect.top, width: rect.width, height: rect.height, side },
          content: { label, shortcut, theme: resolvedTheme },
        }).catch(() => {
          shown.current = false
        })
      }, OPEN_DELAY)
    },
    [label, shortcut, side, resolvedTheme],
  )

  // Unmount with a tooltip up — a route change under the pointer — must take it down.
  React.useEffect(() => hide, [hide])

  return React.useMemo(
    () => ({
      onMouseEnter: (event: React.MouseEvent<HTMLElement>) => {
        show(event.currentTarget)
      },
      onMouseLeave: hide,
      onMouseDown: hide,
      onFocus: (event: React.FocusEvent<HTMLElement>) => {
        show(event.currentTarget)
      },
      onBlur: hide,
    }),
    [show, hide],
  )
}

/** Merges the tip handlers with another set (the animated-icon ones) on one control. */
type Handler = (...args: never[]) => void

export function withHandlers<T extends Record<string, Handler>>(
  first: T,
  second: Partial<Record<keyof T, () => void>>,
): T {
  const merged: Record<string, Handler> = { ...first }
  for (const key of Object.keys(second)) {
    const primary = first[key]
    const extra = second[key]
    merged[key] = (...args: never[]) => {
      primary?.(...args)
      extra?.()
    }
  }
  return merged as T
}
