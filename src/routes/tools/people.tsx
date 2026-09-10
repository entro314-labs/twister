import { IconRadar, IconUsers } from '@tabler/icons-react'
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
  useRun,
} from '@/components/tools/panel'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Select } from '@/components/ui/select'
import {
  useActiveTab,
  useExportPeople,
  useOps,
  usePeople,
  useSiteState,
  useStartOp,
  useStoreCounts,
} from '@/lib/query'
import type { Person, PersonFilter } from '@/lib/tauri/types'
import { cn } from '@/lib/utils'

export const Route = createFileRoute('/tools/people')({ component: PeopleScreen })

type Relation = 'any' | 'notBack' | 'fans' | 'mutual'

const RELATION: Record<Relation, Pick<PersonFilter, 'followsMe' | 'followedByMe'>> = {
  any: {},
  notBack: { followedByMe: true, followsMe: false },
  fans: { followsMe: true, followedByMe: false },
  mutual: { followsMe: true, followedByMe: true },
}

/**
 * People the capture hook has seen, with the filters that answer the usual questions — who does not
 * follow back, who is worth following from a list — and the two operations on them. Scanning a page
 * is how the store fills: open your Following, someone's Followers or a list's members in a tab and
 * press Scan.
 */
function PeopleScreen() {
  const site = useSiteState()
  const tab = useActiveTab()
  const counts = useStoreCounts()
  const ops = useOps()
  const start = useStartOp()
  const exportPeople = useExportPeople()
  const { error, run } = useRun()

  const [search, setSearch] = React.useState('')
  const [source, setSource] = React.useState('')
  const [relation, setRelation] = React.useState<Relation>('any')
  const [minFollowers, setMinFollowers] = React.useState('')
  const [maxFollowers, setMaxFollowers] = React.useState('')
  const [sort, setSort] = React.useState<NonNullable<PersonFilter['sort']>>('seen')
  const [selected, setSelected] = React.useState<Set<string>>(() => new Set())
  const [dryRun, setDryRun] = React.useState(true)
  const [page, setPage] = React.useState('')

  const filter = React.useMemo<PersonFilter>(
    () => ({
      search,
      source: source || undefined,
      ...RELATION[relation],
      minFollowers: minFollowers ? Number(minFollowers) : undefined,
      maxFollowers: maxFollowers ? Number(maxFollowers) : undefined,
      sort,
      limit: 500,
    }),
    [search, source, relation, minFollowers, maxFollowers, sort],
  )
  const people = usePeople(filter)
  const rows = people.data ?? []
  const busy = Boolean(ops.data?.running)
  const currentPath = pathOf(tab?.url)
  const handle = site.data?.handle ?? null

  // The page the follow tools work on: whatever list is open, else the
  // signed-in account's own Following.
  const listPage =
    page ||
    (/\/(?:following|followers|members|verified_followers)/.test(currentPath)
      ? currentPath
      : handle
        ? `/${handle}/following`
        : '')

  const chosen = rows.filter((p) => selected.has(p.id))
  const allChosen = rows.length > 0 && chosen.length === rows.length

  const act = (kind: 'follow' | 'unfollow') => {
    void run(async () =>
      start.mutateAsync({
        kind,
        params: { handles: chosen.map((p) => p.handle), page: listPage },
        dryRun,
      }),
    )
  }

  return (
    <Panel
      title="People"
      note="Everyone X has loaded into a tab while Twister watched: followers, following, list members, search results. Open a list in a tab and scan it to read all of it."
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
          Scrolls the front tab to the end so everything on it is captured.{' '}
          {counts.data ? `${compact(counts.data.users)} people in the store.` : ''}
        </p>
      </Section>

      <Section title="Find">
        <Input
          placeholder="Handle, name or bio"
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
          <Field label="Relationship">
            <Select
              value={relation}
              onChange={(event) => {
                setRelation(event.target.value as Relation)
              }}
            >
              <option value="any">Any</option>
              <option value="notBack">Does not follow back</option>
              <option value="fans">Follows me, I do not</option>
              <option value="mutual">Mutual</option>
            </Select>
          </Field>
          <Field label="Followers from">
            <Input
              inputMode="numeric"
              placeholder="0"
              value={minFollowers}
              onChange={(event) => {
                setMinFollowers(event.target.value.replace(/\D/g, ''))
              }}
            />
          </Field>
          <Field label="Followers up to">
            <Input
              inputMode="numeric"
              placeholder="any"
              value={maxFollowers}
              onChange={(event) => {
                setMaxFollowers(event.target.value.replace(/\D/g, ''))
              }}
            />
          </Field>
          <Field label="Sort">
            <Select
              value={sort}
              onChange={(event) => {
                setSort(event.target.value as NonNullable<PersonFilter['sort']>)
              }}
            >
              <option value="seen">Last seen</option>
              <option value="followers">Followers</option>
              <option value="handle">Handle</option>
            </Select>
          </Field>
          <div className="col-span-2 flex items-end">
            <ExportControls
              disabled={rows.length === 0}
              onExport={(format) => {
                void run(async () => exportPeople.mutateAsync({ filter, format }))
              }}
            />
          </div>
        </div>
      </Section>

      <Section title={`${rows.length} ${rows.length === 1 ? 'person' : 'people'}`}>
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
              <IconUsers className="size-5" />
              Nothing captured with these filters yet.
            </div>
          ) : (
            rows.map((person) => (
              <PersonRow
                key={person.id}
                person={person}
                checked={selected.has(person.id)}
                onToggle={() => {
                  setSelected((previous) => {
                    const next = new Set(previous)
                    if (next.has(person.id)) next.delete(person.id)
                    else next.add(person.id)
                    return next
                  })
                }}
              />
            ))
          )}
        </div>
      </Section>

      <Section title="Act on the selection">
        <Field label="On the list page">
          <Input
            placeholder={listPage || '/handle/following'}
            value={page}
            onChange={(event) => {
              setPage(event.target.value.trim())
            }}
          />
        </Field>
        <p className="text-[11px] leading-relaxed text-muted-foreground">
          Twister opens that page in the front tab and works through its cells, one every few
          seconds, stopping at the first thing X refuses. People not on the page are reported, not
          guessed at. Follow runs cap at 50, unfollow at 100.
        </p>
        <div className="flex flex-wrap items-center gap-2">
          <DryRunToggle dryRun={dryRun} onChange={setDryRun} />
          <Button
            size="sm"
            variant="outline"
            disabled={busy || chosen.length === 0 || !listPage}
            onClick={() => {
              act('follow')
            }}
          >
            Follow {chosen.length || ''}
          </Button>
          <Button
            size="sm"
            variant={dryRun ? 'outline' : 'destructive'}
            disabled={busy || chosen.length === 0 || !listPage}
            onClick={() => {
              act('unfollow')
            }}
          >
            Unfollow {chosen.length || ''}
          </Button>
        </div>
      </Section>
    </Panel>
  )
}

function PersonRow({
  person,
  checked,
  onToggle,
}: {
  person: Person
  checked: boolean
  onToggle: () => void
}) {
  const relation =
    person.followsMe && person.followedByMe
      ? 'mutual'
      : person.followedByMe
        ? person.followsMe === false
          ? 'no follow back'
          : 'following'
        : person.followsMe
          ? 'follows you'
          : ''
  return (
    <label className="flex cursor-pointer items-center gap-2.5 px-2.5 py-2 text-xs">
      <input type="checkbox" checked={checked} onChange={onToggle} />
      {person.avatar ? (
        <img src={person.avatar} alt="" className="size-7 shrink-0 rounded-full" />
      ) : (
        <span className="size-7 shrink-0 rounded-full bg-muted" />
      )}
      <span className="min-w-0 flex-1">
        <span className="flex items-baseline gap-1.5">
          <span className="truncate font-medium">{person.name || person.handle}</span>
          <span className="truncate text-muted-foreground">@{person.handle}</span>
        </span>
        <span className="flex gap-2 text-[11px] text-muted-foreground tabular-nums">
          <span>{compact(person.followers)} followers</span>
          <span>{compact(person.following)} following</span>
          {relation ? (
            <span className={cn(relation === 'no follow back' && 'text-warning')}>{relation}</span>
          ) : null}
        </span>
      </span>
    </label>
  )
}
