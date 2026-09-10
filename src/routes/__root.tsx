import type { QueryClient } from '@tanstack/react-query'
import {
  Outlet,
  createRootRouteWithContext,
  useNavigate,
  useRouterState,
} from '@tanstack/react-router'
import { AnimatePresence, MotionConfig, motion } from 'motion/react'
import * as React from 'react'

import { PaneTitlebar } from '@/components/shell/pane-titlebar'
import { Sidebar } from '@/components/shell/sidebar'
import { StatusBar } from '@/components/shell/status-bar'
import { TOOLS_PANEL_W } from '@/lib/chrome'
import { pageVariants } from '@/lib/motion'
import { PrefsProvider, usePrefs } from '@/lib/prefs'
import { useSettings } from '@/lib/query'
import { useSiteIsland } from '@/lib/site-island'
import { invokeCommand, subscribeEvent } from '@/lib/tauri/client'
import { IPC_COMMANDS, IPC_EVENTS } from '@/lib/tauri/ipc'
import type { ShellAction } from '@/lib/tauri/types'
import { ThemeProvider } from '@/lib/theme'
import { cn } from '@/lib/utils'

interface RouterContext {
  queryClient: QueryClient
}

export const Route = createRootRouteWithContext<RouterContext>()({
  component: RootShell,
})

/** The tool panels sit BESIDE the island; every other screen covers it. */
export const isToolPath = (pathname: string) => pathname.startsWith('/tools')

const TOOL_PATHS: Record<'openPeople' | 'openPosts' | 'openCompose', string> = {
  openPeople: '/tools/people',
  openPosts: '/tools/posts',
  openCompose: '/tools/compose',
}

function RootShell() {
  const settings = useSettings()

  // The window stays hidden until the shell has painted WITH the stored theme,
  // so launch is never a flash of the wrong palette. Two frames: one for React
  // to commit, one for the compositor to draw it.
  const settled = settings.isSuccess || settings.isError
  React.useEffect(() => {
    if (!settled) return
    const frame = requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        void invokeCommand(IPC_COMMANDS.shellReady).catch(() => {})
      })
    })
    return () => {
      cancelAnimationFrame(frame)
    }
  }, [settled])

  return (
    <ThemeProvider settings={settings.data}>
      <PrefsProvider>
        {/* The CSS reduced-motion rule only reaches CSS animations. Motion drives its own, so
            it has to be told about the setting separately. */}
        <MotionConfig reducedMotion="user">
          <ShellActionListener />
          <ShellLayout />
        </MotionConfig>
      </PrefsProvider>
    </ThemeProvider>
  )
}

/**
 * The menu's shell-side verbs. They arrive as events because a menu accelerator fires whichever
 * webview has focus — usually x.com's — and this page is the only one that can act on them. Each
 * tool toggles: asking for the panel that is already open closes it.
 */
function ShellActionListener() {
  const navigate = useNavigate()
  const pathname = useRouterState({ select: (state) => state.location.pathname })
  const { sidebarMode, setSidebarMode } = usePrefs()

  // A ref, so the one subscription sees current values without resubscribing.
  const latest = React.useRef({ pathname, sidebarMode })
  React.useEffect(() => {
    latest.current = { pathname, sidebarMode }
  })

  React.useEffect(() => {
    let detach: (() => void) | null = null
    void (async () => {
      detach = await subscribeEvent<ShellAction>(IPC_EVENTS.shell, (action) => {
        switch (action) {
          case 'openSettings':
            void navigate({ to: latest.current.pathname === '/settings' ? '/' : '/settings' })
            break
          case 'toggleSidebar':
            setSidebarMode(latest.current.sidebarMode === 'full' ? 'rail' : 'full')
            break
          case 'openPeople':
          case 'openPosts':
          case 'openCompose': {
            const to = TOOL_PATHS[action]
            void navigate({ to: latest.current.pathname === to ? '/' : to })
            break
          }
        }
      })
    })()
    return () => {
      detach?.()
    }
  }, [navigate, setSidebarMode])
  return null
}

function ShellLayout() {
  const pathname = useRouterState({ select: (state) => state.location.pathname })
  const tools = isToolPath(pathname)
  // The active tab's webview covers the island exactly, and only while the
  // island is showing X — on its own or beside a tool panel. Every other
  // screen hides it: it sits above this page.
  const islandRef = useSiteIsland(pathname === '/' || tools)

  return (
    // The desk, with a sidebar on it and one island floating above.
    //
    // The DESK is the only translucent surface — it is what an OS effect shows
    // through. The sidebar has no background of its own and sits directly on it.
    // The desk is painted rather than left bare: a webview region with nothing
    // drawn in it does not show the effect behind a transparent window, it
    // renders black.
    <div className="app-desk grid h-screen grid-cols-[auto_1fr] overflow-hidden">
      <Sidebar />
      <div
        className={cn(
          'relative flex min-h-0 min-w-0 flex-col overflow-hidden',
          'bg-[var(--pane-surface)]',
        )}
      >
        <PaneTitlebar />
        <div className="flex min-h-0 flex-1">
          {tools ? (
            <aside
              className="flex min-h-0 shrink-0 flex-col overflow-y-auto border-r border-border/50"
              style={{ width: TOOLS_PANEL_W }}
            >
              <Outlet />
            </aside>
          ) : null}
          <main ref={islandRef} className="relative min-h-0 min-w-0 flex-1 overflow-hidden">
            {tools ? null : (
              <AnimatePresence mode="wait" initial={false}>
                <motion.div
                  // Keyed on the route so the transition plays per destination
                  // rather than once, ever.
                  key={pathname}
                  variants={pageVariants}
                  initial="initial"
                  animate="animate"
                  exit="exit"
                  className="absolute inset-0 overflow-y-auto"
                >
                  <Outlet />
                </motion.div>
              </AnimatePresence>
            )}
          </main>
        </div>
        <StatusBar />
      </div>
    </div>
  )
}
