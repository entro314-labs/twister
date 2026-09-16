import * as React from 'react'

import { NETWORKS, NETWORK_ORDER } from '@/lib/networks'
import { useActiveNetwork } from '@/lib/query'
import type { Network } from '@/lib/tauri/types'
import { cn } from '@/lib/utils'

/**
 * Which network a tool panel is looking at. Follows the front tab until the person picks one, then
 * stays put — a panel that flipped every time a tab changed would lose the selection under them.
 */
export function useNetworkPick(): [Network, (network: Network) => void] {
  const active = useActiveNetwork()
  const [picked, setPicked] = React.useState<Network | null>(null)
  return [picked ?? active, setPicked]
}

/** One segment per network, the chosen one lit. */
export function NetworkPicker({
  value,
  onChange,
  networks = NETWORK_ORDER,
}: {
  value: Network
  onChange: (network: Network) => void
  networks?: readonly Network[]
}) {
  return (
    <div
      role="radiogroup"
      aria-label="Network"
      className="inline-flex self-start rounded-md border border-border/60 bg-muted/40 p-0.5 text-xs"
    >
      {networks.map((network) => {
        const info = NETWORKS[network]
        const Icon = info.icon
        const on = network === value
        return (
          <button
            key={network}
            type="button"
            role="radio"
            aria-checked={on}
            onClick={() => {
              onChange(network)
            }}
            className={cn(
              'inline-flex items-center gap-1.5 rounded-[5px] px-2 py-1 transition-colors',
              on
                ? 'bg-card text-foreground shadow-sm'
                : 'text-muted-foreground hover:text-foreground',
            )}
          >
            <Icon className="size-3.5" stroke={1.75} />
            {info.name}
          </button>
        )
      })}
    </div>
  )
}
