import * as React from 'react'

import { invokeCommand } from '@/lib/tauri/client'
import { IPC_COMMANDS } from '@/lib/tauri/ipc'
import type { Insets } from '@/lib/tauri/types'

/**
 * Keeps the x.com webview glued to the island.
 *
 * The site webview is a sibling of this whole page, positioned by Rust. This hook measures the
 * island element and reports its frame as insets — left, top, right, bottom — rather than a
 * rectangle, so Rust can re-derive the rectangle itself on every window resize without a round
 * trip. The ResizeObserver covers everything else: the sidebar collapsing, a drag on its seam.
 *
 * `visible` hides the webview outright. It sits ON TOP of this page, so anything the shell needs to
 * draw over the island — settings, an error — has to hide the site first.
 */
export function useSiteIsland(visible: boolean) {
  const ref = React.useRef<HTMLElement>(null)

  React.useEffect(() => {
    const element = ref.current
    if (!element) return

    let last: Insets | null = null
    const report = () => {
      const rect = element.getBoundingClientRect()
      const insets: Insets = {
        left: Math.max(0, rect.left),
        top: Math.max(0, rect.top),
        right: Math.max(0, window.innerWidth - rect.right),
        bottom: Math.max(0, window.innerHeight - rect.bottom),
      }
      if (
        last &&
        last.left === insets.left &&
        last.top === insets.top &&
        last.right === insets.right &&
        last.bottom === insets.bottom
      ) {
        return
      }
      last = insets
      void invokeCommand(IPC_COMMANDS.setSiteInsets, { insets }).catch(() => {
        // The webview is gone or Rust is mid-teardown; nothing to draw into.
      })
    }

    report()
    const observer = new ResizeObserver(report)
    observer.observe(element)
    // The island's own size is unchanged by a window resize that only moves
    // it (a maximise on a multi-monitor setup), so listen to the window too.
    window.addEventListener('resize', report)
    return () => {
      observer.disconnect()
      window.removeEventListener('resize', report)
    }
  }, [])

  React.useEffect(() => {
    void invokeCommand(IPC_COMMANDS.setSiteVisible, { visible }).catch(() => {})
  }, [visible])

  return ref
}
