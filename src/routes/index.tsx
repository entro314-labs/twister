import { IconBrandX } from '@tabler/icons-react'
import { createFileRoute } from '@tanstack/react-router'

import { EmptyState } from '@/components/shell/empty-state'

export const Route = createFileRoute('/')({ component: IslandScreen })

/**
 * What the island shows UNDER the site webview. Normally never seen: the webview covers it the
 * moment it exists. If it stays visible, the webview was not created — which is worth saying rather
 * than leaving a blank.
 */
function IslandScreen() {
  return (
    <EmptyState
      icon={IconBrandX}
      title="Opening X"
      description="X loads in its own view over this one. If this message stays, the site view could not be created — quit and open Twister again."
      className="h-full"
    />
  )
}
