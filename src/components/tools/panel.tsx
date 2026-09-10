import { IconPlayerStop } from '@tabler/icons-react'
import * as React from 'react'

import { Button } from '@/components/ui/button'
import { Select } from '@/components/ui/select'
import { useCancelOp, useOps } from '@/lib/query'
import { humanMessage } from '@/lib/tauri/client'
import type { ExportFormat, Job, OpKind } from '@/lib/tauri/types'
import { cn } from '@/lib/utils'

/** The frame every tool panel shares: a heading, a note, then sections. */
export function Panel({
  title,
  note,
  children,
}: {
  title: string
  note: string
  children: React.ReactNode
}) {
  return (
    <div className="flex flex-col gap-5 p-4 pb-8">
      <div>
        <h2 className="font-display text-sm font-semibold tracking-tight">{title}</h2>
        <p className="mt-1 text-xs leading-relaxed text-muted-foreground">{note}</p>
      </div>
      <OpBanner />
      {children}
    </div>
  )
}

export function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="flex flex-col gap-2">
      <h3 className="font-display text-[11px] font-semibold tracking-wide text-muted-foreground uppercase">
        {title}
      </h3>
      {children}
    </section>
  )
}

export function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <label className="flex flex-col gap-1 text-xs text-muted-foreground">
      <span>{label}</span>
      {children}
    </label>
  )
}

export function ErrorLine({ message }: { message: string | null }) {
  if (!message) return null
  return (
    <p className="rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-xs text-destructive">
      {message}
    </p>
  )
}

/** Runs a mutation and keeps its failure as a line of text. */
export function useRun() {
  const [error, setError] = React.useState<string | null>(null)
  const run = React.useCallback(async (action: () => Promise<unknown>) => {
    try {
      await action()
      setError(null)
    } catch (err) {
      setError(humanMessage(err))
    }
  }, [])
  return { error, run, setError }
}

const KIND_LABEL: Record<OpKind, string> = {
  scan: 'Scan',
  follow: 'Follow',
  unfollow: 'Unfollow',
  delete: 'Delete',
  compose: 'Post',
}

const STATUS_LABEL: Record<Job['status'], string> = {
  queued: 'queued',
  running: 'running',
  done: 'done',
  failed: 'failed',
  cancelled: 'stopped',
}

/**
 * The operation running in the front tab, or the last one that ran. Operations happen in the
 * island, in view; this is the ledger line for them, with the one control that matters.
 */
function OpBanner() {
  const ops = useOps()
  const cancel = useCancelOp()
  const running = ops.data?.running ?? null
  const last = ops.data?.recent[0] ?? null
  const job = running ?? last
  if (!job) return null

  const failed = job.status === 'failed'
  return (
    <div
      className={cn(
        'flex items-start gap-2 rounded-md border px-3 py-2 text-xs',
        running
          ? 'border-warning/40 bg-warning/10'
          : failed
            ? 'border-destructive/30 bg-destructive/10'
            : 'border-border/60 bg-card/60',
      )}
    >
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-1.5 font-medium">
          {running ? (
            <span className="size-1.5 animate-[var(--animate-carrier)] rounded-full bg-warning" />
          ) : null}
          <span>
            {KIND_LABEL[job.kind]}
            {job.dryRun ? ' · dry run' : ''}
            {running ? '' : ` · ${STATUS_LABEL[job.status]}`}
          </span>
          {job.total > 0 || job.done > 0 ? (
            <span className="text-muted-foreground tabular-nums">
              {job.done}
              {job.total > 0 ? `/${job.total}` : ''}
              {job.skipped > 0 ? ` · ${job.skipped} skipped` : ''}
              {job.failed > 0 ? ` · ${job.failed} failed` : ''}
            </span>
          ) : null}
        </div>
        {job.message ? (
          <p className="mt-0.5 truncate text-muted-foreground" title={job.message}>
            {job.message}
          </p>
        ) : null}
      </div>
      {running ? (
        <Button
          size="xs"
          variant="destructive"
          onClick={() => {
            cancel.mutate()
          }}
        >
          <IconPlayerStop data-icon="inline-start" className="size-3" />
          Stop
        </Button>
      ) : null}
    </div>
  )
}

export function ExportControls({
  disabled,
  onExport,
}: {
  disabled: boolean
  onExport: (format: ExportFormat) => void
}) {
  const [format, setFormat] = React.useState<ExportFormat>('csv')
  return (
    <div className="flex items-center gap-1.5">
      <Select
        value={format}
        onChange={(event) => {
          setFormat(event.target.value as ExportFormat)
        }}
        className="w-28"
        aria-label="Export format"
      >
        <option value="csv">CSV</option>
        <option value="json">JSON</option>
        <option value="markdown">Markdown</option>
      </Select>
      <Button
        size="sm"
        variant="outline"
        disabled={disabled}
        onClick={() => {
          onExport(format)
        }}
      >
        Export…
      </Button>
    </div>
  )
}

/** A dry-run switch drawn as two segments, so the live half is a deliberate choice. */
export function DryRunToggle({
  dryRun,
  onChange,
}: {
  dryRun: boolean
  onChange: (dryRun: boolean) => void
}) {
  return (
    <div
      role="radiogroup"
      aria-label="Run mode"
      className="inline-flex rounded-md border border-border/60 bg-muted/40 p-0.5 text-xs"
    >
      {[
        { value: true, label: 'Dry run' },
        { value: false, label: 'For real' },
      ].map((option) => (
        <button
          key={option.label}
          type="button"
          role="radio"
          aria-checked={dryRun === option.value}
          onClick={() => {
            onChange(option.value)
          }}
          className={cn(
            'rounded-[5px] px-2 py-1 transition-colors',
            dryRun === option.value
              ? option.value
                ? 'bg-card text-foreground shadow-sm'
                : 'bg-destructive/15 text-destructive shadow-sm'
              : 'text-muted-foreground hover:text-foreground',
          )}
        >
          {option.label}
        </button>
      ))}
    </div>
  )
}

export function pathOf(url: string | undefined): string {
  if (!url) return ''
  try {
    return new URL(url).pathname
  } catch {
    return ''
  }
}

export function compact(n: number): string {
  return new Intl.NumberFormat('en', { notation: 'compact', maximumFractionDigits: 1 }).format(n)
}

export function shortDate(iso: string): string {
  if (!iso) return ''
  const date = new Date(iso)
  if (Number.isNaN(date.getTime())) return ''
  return date.toLocaleDateString(undefined, { year: 'numeric', month: 'short', day: 'numeric' })
}
