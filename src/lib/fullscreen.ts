import { getCurrentWindow } from '@tauri-apps/api/window'
import * as React from 'react'

/**
 * Whether the window is in native fullscreen.
 *
 * Only the macOS traffic-light clearance needs this: fullscreen hides the lights, and a header that
 * keeps reserving room for buttons that are not there reads as a typo. There is no fullscreen event
 * to listen for, so it is re-read on every resize — entering or leaving fullscreen always resizes
 * the window, and the shell is a child webview that resizes with it. The DOM's own event rather
 * than Tauri's for the same reason `site-island.ts` uses it: it needs no listener handshake.
 */
export function useIsFullscreen(): boolean {
  const [isFullscreen, setIsFullscreen] = React.useState(false)

  React.useEffect(() => {
    const appWindow = getCurrentWindow()
    let cancelled = false

    const sync = async () => {
      try {
        const value = await appWindow.isFullscreen()
        if (!cancelled) setIsFullscreen(value)
      } catch {
        // Mid-teardown; the last value is as good as any.
      }
    }

    void sync()
    const onResize = () => {
      void sync()
    }
    window.addEventListener('resize', onResize)
    return () => {
      cancelled = true
      window.removeEventListener('resize', onResize)
    }
  }, [])

  return isFullscreen
}
