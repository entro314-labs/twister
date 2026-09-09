import * as React from 'react'

import { cn } from '@/lib/utils'

/** Inline keycap, for the shortcuts listed in Settings and the sidebar tooltips. */
function Kbd({ className, ...props }: React.ComponentProps<'kbd'>) {
  return (
    <kbd
      data-slot="kbd"
      className={cn(
        'inline-flex h-5 min-w-5 items-center justify-center rounded-[5px]',
        'border border-border/60 bg-muted/70 px-1.5 text-[10px] font-medium',
        'text-muted-foreground shadow-[inset_0_-1px_0_0_oklch(0_0_0_/_0.08)]',
        'font-mono leading-none tabular-nums',
        className,
      )}
      {...props}
    />
  )
}

export { Kbd }
