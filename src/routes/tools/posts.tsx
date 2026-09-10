import { IconNotes, IconPhoto, IconRadar } from '@tabler/icons-react'
import { createFileRoute } from '@tanstack/react-router'
import * as React from 'react'

import {
  DryRunToggle,
  ErrorLine,
  ExportControls,
  Field,
  Panel,
  Section,
  compact,
  pathOf,
  shortDate,
  useRun,
} from '@/components/tools/panel'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Select } from '@/components/ui/select'
import {
  useActiveTab,
  useExportPosts,
  useOps,
  usePosts,
  useSiteState,
  useStartOp,
  useStoreCounts,
} from '@/lib/query'
import type { Post, PostFilter, PostKind } from '@/lib/tauri/types'

export const Route = createFileRoute('/tools/posts')({ component: PostsScreen })

const KIND_LABEL: Record<PostKind, string> = {
  post: 'post',
  reply: 'reply',
  repost: 'repost',
  quote: 'quote',
}

/**
 * Posts the capture hook has seen: a timeline, a profile, a list, bookmarks. Filter, export, and —
 * for your own — delete in bulk. Bookmarks export is this screen with "Seen in" set to Bookmarks
 * after a scan of the bookmarks page.
 */
function PostsScreen() {
  const site = useSiteState()
  const tab = useActiveTab()
  const counts = useStoreCounts()
  const ops = useOps()
  const start = useStartOp()
  const exportPosts = useExportPosts()
  const { error, run } = useRun()

  const [search, setSearch] = React.useState('')
  const [source, setSource] = React.useState('')
  const [kind, setKind] = React.useState<'' | PostKind>('')
  const [author, setAuthor] = React.useState('')
  const [media, setMedia] = React.useState<'' | 'yes' | 'no'>('')
  const [sort, setSort] = React.useState<NonNullable<PostFilter['sort']>>('seen')
  const [selected, setSelected] = React.useState<Set<string>>(() => new Set())
  const [dryRun, setDryRun] = React.useState(true)

  const filter = React.useMemo<PostFilter>(
    () => ({
      search,
      source: source || undefined,
      kind: kind || undefined,
      author: author || undefined,
      hasMedia: media === '' ? undefined : media === 'yes',
      sort,
      limit: 500,
    }),
    [search, source, kind, author, media, sort],
  )
  const posts = usePosts(filter)
  const rows = posts.data ?? []
  const busy = Boolean(ops.data?.running)
  const currentPath = pathOf(tab?.url)
  const handle = site.data?.handle ?? null

  const chosen = rows.filter((p) => selected.has(p.id))
  const allChosen = rows.length > 0 && chosen.length === rows.length
  const allMine =
    Boolean(handle) &&
    chosen.length > 0 &&
    chosen.every((p) => p.authorHandle.toLowerCase() === handle?.toLowerCase())
  const likesPage = /\/likes\/?$/.test(currentPath)
  // Deleting happens on your profile, or on your Likes page for unliking.
  const deletePage = likesPage && handle ? `/${handle}/likes` : handle ? `/${handle}` : ''

  return (
    <Panel
      title="Posts"
      note="Every post X has loaded into a tab while Twister watched, with what X said about it. Open your bookmarks, a profile or a list in a tab and scan it to read all of it."
    >
      <ErrorLine message={error} />

      <Section title="Fill the store">
        <div className="flex items-center gap-2">
          <Button
            size="sm"
            disabled={busy || !currentPath}
            onClick={() => {
              void run(async () =>
                start.mutateAsync({ kind: 'scan', params: { page: currentPath }, dryRun: false }),
              )
            }}
          >
            <IconRadar data-icon="inline-start" />
            Scan this page
          </Button>
          <span className="truncate font-mono text-[11px] text-muted-foreground">
            {currentPath || 'no tab'}
          </span>
        </div>
        <p className="text-[11px] leading-relaxed text-muted-foreground">
          {counts.data ? `${compact(counts.data.posts)} posts in the store. ` : ''}
          For bookmarks, open Bookmarks (⌘5), scan, then pick Bookmarks below.
        </p>
      </Section>

      <Section title="Find">
        <Input
          placeholder="Words in the post"
          value={search}
          onChange={(event) => {
            setSearch(event.target.value)
          }}
        />
        <div className="grid grid-cols-2 gap-2">
          <Field label="Seen in">
            <Select
              value={source}
              onChange={(event) => {
                setSource(event.target.value)
              }}
            >
              <option value="">Anywhere</option>
              {(counts.data?.sources ?? []).map(([name, n]) => (
                <option key={name} value={name}>
                  {name} ({n})
                </option>
              ))}
            </Select>
          </Field>
          <Field label="Kind">
            <Select
              value={kind}
              onChange={(event) => {
                setKind(event.target.value as '' | PostKind)
              }}
            >
              <option value="">Any</option>
              <option value="post">Posts</option>
              <option value="reply">Replies</option>
              <option value="repost">Reposts</option>
              <option value="quote">Quotes</option>
            </Select>
          </Field>
          <Field label="By">
            <Input
              placeholder={handle ? `@${handle}` : '@handle'}
              value={author}
              onChange={(event) => {
                setAuthor(event.target.value.trim())
              }}
            />
          </Field>
          <Field label="Media">
            <Select
              value={media}
              onChange={(event) => {
                setMedia(event.target.value as '' | 'yes' | 'no')
              }}
            >
              <option value="">Either</option>
              <option value="yes">With photos or video</option>
              <option value="no">Text only</option>
            </Select>
          </Field>
          <Field label="Sort">
            <Select
              value={sort}
              onChange={(event) => {
                setSort(event.target.value as NonNullable<PostFilter['sort']>)
              }}
            >
              <option value="seen">Last seen</option>
              <option value="created">Newest</option>
              <option value="likes">Most liked</option>
            </Select>
          </Field>
          <div className="col-span-2 flex items-end">
            <ExportControls
              disabled={rows.length === 0}
              onExport={(format) => {
                void run(async () => exportPosts.mutateAsync({ filter, format }))
              }}
            />
          </div>
        </div>
      </Section>

      <Section title={`${rows.length} ${rows.length === 1 ? 'post' : 'posts'}`}>
        <label className="flex items-center gap-2 px-1 text-xs text-muted-foreground">
          <input
            type="checkbox"
            checked={allChosen}
            onChange={() => {
              setSelected(allChosen ? new Set() : new Set(rows.map((p) => p.id)))
            }}
          />
          {chosen.length ? `${chosen.length} selected` : 'Select all'}
        </label>
        <div className="divide-y divide-border/50 rounded-lg border border-border/60 bg-card/60">
          {rows.length === 0 ? (
            <div className="flex flex-col items-center gap-2 px-4 py-8 text-center text-xs text-muted-foreground">
              <IconNotes className="size-5" />
              Nothing captured with these filters yet.
            </div>
          ) : (
            rows.map((post) => (
              <PostRow
                key={post.id}
                post={post}
                checked={selected.has(post.id)}
                onToggle={() => {
                  setSelected((previous) => {
                    const next = new Set(previous)
                    if (next.has(post.id)) next.delete(post.id)
                    else next.add(post.id)
                    return next
                  })
                }}
              />
            ))
          )}
        </div>
      </Section>

      <Section title="Delete the selection">
        <p className="text-[11px] leading-relaxed text-muted-foreground">
          Only your own posts, from your profile page: Twister opens it in the front tab, finds each
          selected post as it scrolls, and deletes it through X’s own menu — reposts are undone, and
          on your Likes page the selection is unliked instead. One every few seconds; stops at the
          first thing X refuses.
          {handle ? '' : ' Twister has not seen your handle yet.'}
          {chosen.length > 0 && !allMine ? ' The selection includes posts that are not yours.' : ''}
        </p>
        <div className="flex flex-wrap items-center gap-2">
          <DryRunToggle dryRun={dryRun} onChange={setDryRun} />
          <Button
            size="sm"
            variant={dryRun ? 'outline' : 'destructive'}
            disabled={busy || !allMine || !deletePage}
            onClick={() => {
              void run(async () =>
                start.mutateAsync({
                  kind: 'delete',
                  params: { ids: chosen.map((p) => p.id), page: deletePage },
                  dryRun,
                }),
              )
            }}
          >
            {likesPage ? 'Unlike' : 'Delete'} {chosen.length || ''}
          </Button>
        </div>
      </Section>
    </Panel>
  )
}

function PostRow({
  post,
  checked,
  onToggle,
}: {
  post: Post
  checked: boolean
  onToggle: () => void
}) {
  return (
    <label className="flex cursor-pointer items-start gap-2.5 px-2.5 py-2 text-xs">
      <input
        type="checkbox"
        checked={checked}
        onChange={onToggle}
        className="mt-0.5"
        aria-label={`Select post by @${post.authorHandle}`}
      />
      <span className="min-w-0 flex-1">
        <span className="flex items-baseline gap-1.5 text-[11px] text-muted-foreground">
          <span className="truncate font-medium text-foreground">@{post.authorHandle}</span>
          <span>{shortDate(post.createdAt)}</span>
          {post.kind !== 'post' ? <span>{KIND_LABEL[post.kind]}</span> : null}
          {post.media.length ? (
            <span className="inline-flex items-center gap-0.5">
              <IconPhoto className="size-3" />
              {post.media.length}
            </span>
          ) : null}
          <span className="ml-auto tabular-nums">{compact(post.likes)} ♥</span>
        </span>
        <span className="line-clamp-3 leading-relaxed break-words whitespace-pre-line">
          {post.text}
        </span>
      </span>
    </label>
  )
}
