import * as React from 'react'

import { cn } from '@/lib/utils'

/** A native text input, styled to match the select. */
function Input({ className, ...props }: React.ComponentProps<'input'>) {
  return (
    <input
      data-slot="input"
      className={cn(
        'h-8 w-full min-w-0 rounded-md border border-input bg-background/40 px-2.5 text-sm outline-none',
        'shadow-[inset_0_1px_0_0_oklch(0_0_0_/_0.04)] placeholder:text-muted-foreground/70',
        'transition-[background-color,border-color,box-shadow] duration-150',
        'hover:border-input/80 focus-visible:border-ring/60 focus-visible:ring-3 focus-visible:ring-ring/30',
        'disabled:pointer-events-none disabled:opacity-50',
        'dark:bg-input/20 dark:shadow-[inset_0_1px_0_0_oklch(1_0_0_/_0.04)]',
        className,
      )}
      {...props}
    />
  )
}

function Textarea({ className, ...props }: React.ComponentProps<'textarea'>) {
  return (
    <textarea
      data-slot="textarea"
      className={cn(
        'w-full min-w-0 rounded-md border border-input bg-background/40 px-2.5 py-2 text-sm outline-none',
        'shadow-[inset_0_1px_0_0_oklch(0_0_0_/_0.04)] placeholder:text-muted-foreground/70',
        'transition-[background-color,border-color,box-shadow] duration-150',
        'hover:border-input/80 focus-visible:border-ring/60 focus-visible:ring-3 focus-visible:ring-ring/30',
        'disabled:pointer-events-none disabled:opacity-50',
        'dark:bg-input/20 dark:shadow-[inset_0_1px_0_0_oklch(1_0_0_/_0.04)]',
        className,
      )}
      {...props}
    />
  )
}

export { Input, Textarea }
