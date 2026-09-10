import { QueryClient } from '@tanstack/react-query'

import { errorCode } from '@/lib/tauri/client'

/**
 * One shared QueryClient. Reads are cheap (in-memory state over IPC) and Rust pushes every change
 * through the event bridge, so nothing here ever polls.
 */
export const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 20_000,
      gcTime: 5 * 60_000,
      retry: (failureCount, error) => {
        // These are the codes nothing fixes by trying again: a rejected input,
        // a webview that is gone, a channel with no release, a releases
        // repository that is not answering. Retrying only delays the message —
        // the update surfaces offer the user their own Retry instead.
        const code = errorCode(error)
        if (
          code === 'NOT_FOUND' ||
          code === 'INVALID_INPUT' ||
          code === 'NO_RELEASE' ||
          code === 'UPDATE_SOURCE_UNREACHABLE'
        ) {
          return false
        }
        return failureCount < 2
      },
      retryDelay: (attempt) => Math.min(1000 * 2 ** attempt, 8000),
      refetchOnWindowFocus: false,
    },
    mutations: { retry: 0 },
  },
})
