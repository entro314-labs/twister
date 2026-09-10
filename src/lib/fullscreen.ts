import { getCurrentWindow } from '@tauri-apps/api/window'
import * as React from 'react'

/**
 * Whether the window is in native fullscreen.
 *
 * Only the macOS traffic-light clearance needs this: fullscreen hides the lights, and a header that
 * keeps reserving room for buttons that are not there reads as a typo. Tauri's window API has no
 * fullscreen event, so it is derived from the resize and move the transition produces.
 */
export function useIsFullscreen(): boolean {
  const [isFullscreen, setIsFullscreen] = React.useState(false)

  React.useEffect(() => {
    const appWindow = getCurrentWindow()
    let cancelled = false
    const unlisteners: Array<() => void> = []

    const sync = async () => {
      try {
        const value = await appWindow.isFullscreen()
        if (!cancelled) setIsFullscreen(value)
      } catch {
        // Mid-teardown; the last value is as good as any.
      }
    }

    const attach = async () => {
      try {
        const stops = await Promise.all([appWindow.onResized(sync), appWindow.onMoved(sync)])
        if (cancelled) for (const stop of stops) stop()
        else unlisteners.push(...stops)
      } catch {
        // No window to listen to; the initial read is all there is.
      }
    }

    void sync()
    void attach()

    return () => {
      cancelled = true
      for (const unlisten of unlisteners) unlisten()
    }
  }, [])

  return isFullscreen
}
