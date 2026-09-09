import './index.css'

import { QueryClientProvider } from '@tanstack/react-query'
import { RouterProvider, createRouter } from '@tanstack/react-router'
import React from 'react'
import ReactDOM from 'react-dom/client'

import { NotFoundScreen, RouteErrorScreen } from '@/components/shell/error-screen'
import { attachEventBridge, queryClient } from '@/lib/query'

import { routeTree } from './routeTree.gen'

// Kept in module scope so Vite's HMR can detach between hot reloads instead of
// stacking a new listener on every save.
let detachEvents: (() => void) | null = null

const router = createRouter({
  routeTree,
  context: { queryClient },
  defaultPreload: 'intent',
  defaultPreloadStaleTime: 0,
  defaultErrorComponent: RouteErrorScreen,
  defaultNotFoundComponent: NotFoundScreen,
})

declare module '@tanstack/react-router' {
  interface Register {
    router: typeof router
  }
}

const root = document.getElementById('root')
if (!root) throw new Error('index.html is missing #root')

// Rendered immediately: first paint must not wait on IPC. The event bridge
// attaches in the background — the site state is fetched once on its own, and
// the bridge only adds the pushes on top.
ReactDOM.createRoot(root).render(
  <React.StrictMode>
    <QueryClientProvider client={queryClient}>
      <RouterProvider router={router} />
    </QueryClientProvider>
  </React.StrictMode>,
)

async function connectEventBridge() {
  try {
    detachEvents = await attachEventBridge(queryClient)
  } catch {
    // Without the bridge the sidebar's active row and the unread count stop
    // following the page. The site itself is unaffected, so this is logged
    // rather than shown: the status bar has nothing useful to say about it.
  }
}
void connectEventBridge()

if (import.meta.hot) {
  import.meta.hot.dispose(() => {
    detachEvents?.()
    detachEvents = null
  })
}
