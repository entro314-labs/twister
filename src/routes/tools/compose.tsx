import { IconCalendarClock, IconSend, IconTrash } from '@tabler/icons-react'
import { createFileRoute } from '@tanstack/react-router'
import * as React from 'react'

import { ErrorLine, Field, Panel, Section, useRun } from '@/components/tools/panel'
import { Button } from '@/components/ui/button'
import { Input, Textarea } from '@/components/ui/input'
import {
  useDeleteScheduledPost,
  useOps,
  usePostNow,
  usePreparePost,
  useSchedulePost,
  useScheduledPosts,
  useSiteState,
} from '@/lib/query'
import type { ScheduledPost } from '@/lib/tauri/types'
import { cn } from '@/lib/utils'

export const Route = createFileRoute('/tools/compose')({ component: ComposeScreen })

const DRAFT_KEY = 'twister.draft'

function readDraft(): string {
  try {
    return window.localStorage.getItem(DRAFT_KEY) ?? ''
  } catch {
    return ''
  }
}

/** A `datetime-local` value for the next round quarter hour, as a default. */
function nextQuarter(): string {
  const date = new Date(Date.now() + 15 * 60_000)
  date.setMinutes(Math.ceil(date.getMinutes() / 15) * 15, 0, 0)
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(date.getHours())}:${pad(date.getMinutes())}`
}

const STATUS_STYLE: Record<ScheduledPost['status'], string> = {
  scheduled: 'text-primary',
  posting: 'text-warning',
  posted: 'text-success',
  failed: 'text-destructive',
  missed: 'text-destructive',
}

/**
 * Markdown in, a thread out. The preview is Rust's own rendering — the same split and the same
 * count the post will go out with — so what it shows is what X gets. Posting goes through X's own
 * composer in the front tab, typed in; scheduling waits for the app to be open at the time.
 */
function ComposeScreen() {
  const site = useSiteState()
  const ops = useOps()
  const postNow = usePostNow()
  const schedule = useSchedulePost()
  const scheduled = useScheduledPosts()
  const remove = useDeleteScheduledPost()
  const { error, run } = useRun()

  const [markdown, setMarkdown] = React.useState(readDraft)
  const [debounced, setDebounced] = React.useState(markdown)
  const [when, setWhen] = React.useState(nextQuarter)

  React.useEffect(() => {
    const timer = setTimeout(() => {
      setDebounced(markdown)
    }, 150)
    try {
      window.localStorage.setItem(DRAFT_KEY, markdown)
    } catch {
      // Losing a draft on a blocked store is not worth surfacing.
    }
    return () => {
      clearTimeout(timer)
    }
  }, [markdown])

  const prepared = usePreparePost(debounced)
  const parts = prepared.data?.parts ?? []
  const limit = prepared.data?.limit ?? 280
  const over = parts.some((part) => part.count > limit)
  const busy = Boolean(ops.data?.running)
  const signedIn = Boolean(site.data?.handle)
  const empty = parts.length === 0

  return (
    <Panel
      title="Write"
      note="Markdown that survives X: **bold** and *italic* become styled letters, `code` monospace, lists bullets, links plain. A line with --- breaks the thread; anything over 280 is split at a sentence."
    >
      <ErrorLine message={error} />

      <Section title="Draft">
        <Textarea
          value={markdown}
          onChange={(event) => {
            setMarkdown(event.target.value)
          }}
          rows={8}
          placeholder="What is happening?"
          className="font-mono text-[13px] leading-relaxed"
          spellCheck
        />
      </Section>

      <Section title={parts.length > 1 ? `Thread of ${parts.length}` : 'Preview'}>
        {empty ? (
          <p className="px-1 text-xs text-muted-foreground">Nothing to post yet.</p>
        ) : (
          <ol className="flex flex-col gap-2">
            {parts.map((part, index) => (
              <li
                // Parts have no identity of their own; position is what they are.
                // oxlint-disable-next-line react/no-array-index-key
                key={index}
                className="rounded-lg border border-border/60 bg-card/60 px-3 py-2 text-sm leading-relaxed break-words whitespace-pre-wrap"
              >
                {part.text}
                <div
                  className={cn(
                    'mt-1 text-right text-[11px] tabular-nums',
                    part.count > limit ? 'text-destructive' : 'text-muted-foreground',
                  )}
                >
                  {part.count}/{limit}
                </div>
              </li>
            ))}
          </ol>
        )}
        <p className="text-[11px] leading-relaxed text-muted-foreground">
          Styled letters are Unicode look-alikes: they read as bold on X and as gibberish to a
          screen reader. Use them sparingly.
        </p>
      </Section>

      <Section title="Send">
        <div className="flex flex-wrap items-center gap-2">
          <Button
            size="sm"
            disabled={busy || empty || over || !signedIn}
            onClick={() => {
              void run(async () => {
                await postNow.mutateAsync(markdown)
                setMarkdown('')
              })
            }}
          >
            <IconSend data-icon="inline-start" />
            Post now
          </Button>
          <span className="text-[11px] text-muted-foreground">
            {signedIn ? 'Types it into X’s composer in the front tab.' : 'Sign in to X first.'}
          </span>
        </div>
        <div className="flex flex-wrap items-end gap-2">
          <Field label="Or at">
            <Input
              type="datetime-local"
              value={when}
              onChange={(event) => {
                setWhen(event.target.value)
              }}
              className="w-52"
            />
          </Field>
          <Button
            size="sm"
            variant="outline"
            disabled={empty || over || !when}
            onClick={() => {
              void run(async () => {
                const date = new Date(when)
                if (Number.isNaN(date.getTime())) throw new Error('That is not a time.')
                await schedule.mutateAsync({ markdown, scheduledAt: date.toISOString() })
                setMarkdown('')
              })
            }}
          >
            <IconCalendarClock data-icon="inline-start" />
            Schedule
          </Button>
        </div>
        <p className="text-[11px] leading-relaxed text-muted-foreground">
          A scheduled post goes out only while Twister is open and signed in. More than 15 minutes
          late — the machine was asleep — and it is marked missed rather than sent late.
        </p>
      </Section>

      <Section title="Scheduled">
        <div className="divide-y divide-border/50 rounded-lg border border-border/60 bg-card/60">
          {(scheduled.data ?? []).length === 0 ? (
            <p className="px-3 py-4 text-center text-xs text-muted-foreground">
              Nothing scheduled.
            </p>
          ) : (
            (scheduled.data ?? []).map((post) => (
              <div key={post.id} className="flex items-start gap-2 px-3 py-2 text-xs">
                <div className="min-w-0 flex-1">
                  <div className="flex items-baseline gap-2 text-[11px]">
                    <span className={cn('font-medium', STATUS_STYLE[post.status])}>
                      {post.status}
                    </span>
                    <span className="text-muted-foreground">
                      {new Date(post.scheduledAt).toLocaleString()}
                    </span>
                    {post.parts.length > 1 ? (
                      <span className="text-muted-foreground">{post.parts.length} parts</span>
                    ) : null}
                  </div>
                  <p className="mt-0.5 line-clamp-2 break-words text-foreground/90">
                    {post.parts[0]}
                  </p>
                  {post.error ? <p className="mt-0.5 text-destructive">{post.error}</p> : null}
                </div>
                {post.status !== 'posting' ? (
                  <Button
                    size="icon-xs"
                    variant="ghost"
                    aria-label="Remove"
                    onClick={() => {
                      void run(async () => remove.mutateAsync(post.id))
                    }}
                  >
                    <IconTrash className="size-3.5" />
                  </Button>
                ) : null}
              </div>
            ))
          )}
        </div>
      </Section>
    </Panel>
  )
}
