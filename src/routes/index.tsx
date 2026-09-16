import { createFileRoute } from '@tanstack/react-router'

import { EmptyState } from '@/components/shell/empty-state'
import { NETWORKS } from '@/lib/networks'
import { useActiveNetwork } from '@/lib/query'

export const Route = createFileRoute('/')({ component: IslandScreen })

/**
 * What the island shows UNDER the site webview. Normally never seen: the webview covers it the
 * moment it exists. If it stays visible, the webview was not created — which is worth saying rather
 * than leaving a blank.
 */
function IslandScreen() {
  const network = NETWORKS[useActiveNetwork()]
  return (
    <EmptyState
      icon={network.icon}
      title={`Opening ${network.name}`}
      description={`${network.name} loads in its own view over this one. If this message stays, the site view could not be created — quit and open Twister again.`}
      className="h-full"
    />
  )
}
