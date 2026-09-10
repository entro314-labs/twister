import { createFileRoute } from '@tanstack/react-router'
import { getVersion } from '@tauri-apps/api/app'
import * as React from 'react'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Kbd } from '@/components/ui/kbd'
import { Select } from '@/components/ui/select'
import { Switch } from '@/components/ui/switch'
import { CLIENT_ACTIONS, RATE_CARD_CHECKED, SELF_SERVE_CUT, dayCost, usd } from '@/lib/api-costs'
import { MOD_KEY } from '@/lib/chrome'
import {
  useClearCaptured,
  useOpenDownloadsDir,
  useOpenUserAssetsDir,
  useReloadSite,
  useSettings,
  useSignOut,
  useSiteState,
  useStoreCounts,
  useUpdateSettings,
  useUserAssets,
} from '@/lib/query'
import { humanMessage } from '@/lib/tauri/client'
import type { Niceties, Settings } from '@/lib/tauri/types'

export const Route = createFileRoute('/settings')({ component: SettingsScreen })

/**
 * Every nicety, with what it does in plain words. Each one is a selector on X's own page, so the
 * hint says what disappears — and the section note says why one might stop working.
 */
const NICETIES: Array<{ key: keyof Niceties; label: string; hint: string }> = [
  {
    key: 'chronologicalHome',
    label: 'Following first',
    hint: 'Open the home timeline on Following rather than For you. Once per visit — switching tabs yourself still works.',
  },
  {
    key: 'hidePromoted',
    label: 'Hide promoted posts',
    hint: 'Drops ads from every timeline.',
  },
  {
    key: 'hideRightColumn',
    label: 'Hide the right column',
    hint: 'Trends, who to follow, and the premium upsells.',
  },
  {
    key: 'hideExtrasNav',
    label: 'Trim X’s navigation',
    hint: 'Grok, Premium, Jobs and the other entries that are not your timeline.',
  },
  {
    key: 'hideViewCounts',
    label: 'Hide view counts',
    hint: 'The number under each post that only X cares about.',
  },
  {
    key: 'hideSiteNav',
    label: 'Hide X’s navigation entirely',
    hint: 'Twister’s sidebar and the View menu carry the same destinations. X’s own compose button and account menu go with it; Sign out lives in Settings.',
  },
  {
    key: 'hideDrawers',
    label: 'Hide the floating drawers',
    hint: 'The Grok and Messages panels X pins to the bottom-right corner.',
  },
  {
    key: 'classicTwitter',
    label: 'Twitter',
    hint: 'One switch: the blue bird in place of the X mark, the classic blue on Post and Follow, and posts called tweets again in X’s own buttons, tabs and titles (English only).',
  },
  {
    key: 'fitTimeline',
    label: 'Fit the timeline',
    hint: 'Lets X’s 600px column grow to fill the island. Twister also keeps the window wide enough for X’s layout, right column included when it shows.',
  },
  {
    key: 'smoothScroll',
    label: 'Smooth scrolling',
    hint: 'Animated scrolling on keyboard and programmatic jumps. Refresh rate is the display’s own; the site view already draws at it.',
  },
  {
    key: 'downloadButton',
    label: 'Download button on posts',
    hint: 'A button in each post’s action bar that saves its photos or video, at full size, to Downloads/Twister.',
  },
  {
    key: 'capture',
    label: 'Remember what X loads',
    hint: 'Keeps the people and posts X loads into a tab in a local store, for the People and Posts tools. Nothing is fetched; nothing leaves this machine.',
  },
  {
    key: 'dim',
    label: 'Dim',
    hint: 'X’s retired blue-grey dark theme, painted over Lights out. Set X itself to Lights out for it to take.',
  },
  {
    key: 'dockBadge',
    label: 'Unread count on the Dock icon',
    hint: 'Mirrors the count X puts in the page title.',
  },
]

/**
 * The look: posts and the composer drawn the way a classic client drew them. The same contract as
 * the niceties — one selector each, one switch each.
 */
const LOOK: Array<{ key: keyof Niceties; label: string; hint: string }> = [
  {
    key: 'compactPosts',
    label: 'Compact posts',
    hint: 'A 32px avatar, a smaller byline, and the action bar pulled up under the text.',
  },
  {
    key: 'squareAvatars',
    label: 'Rounded-square avatars',
    hint: 'Squares with soft corners in place of circles, everywhere X draws one.',
  },
  {
    key: 'actionsOnHover',
    label: 'Actions on hover',
    hint: 'A post’s reply, repost, like and share buttons show only while the post is under the pointer or has focus. The post on its own page keeps them.',
  },
  {
    key: 'hideActionCounts',
    label: 'No counts on the action bar',
    hint: 'The numbers beside reply, repost and like go; the post’s own page still has them.',
  },
  {
    key: 'starFavorites',
    label: 'Stars, not hearts',
    hint: 'A star for the like button, gold when lit, and likes called favorites in X’s own controls (English only).',
  },
  {
    key: 'compactCompose',
    label: 'A quieter composer',
    hint: 'The audience chip, the who-can-reply line and Grok go; the count is a number of characters left, from X’s own sum, in place of the ring.',
  },
  {
    key: 'hideInlineComposer',
    label: 'No composer in the timeline',
    hint: 'The “What is happening?” box at the top of Home goes. New post in the titlebar, or ⌘N, opens the composer as its own sheet.',
  },
  {
    key: 'hidePageHeaders',
    label: 'Hide X’s page headers',
    hint: 'The sticky title at the top of the column — Home and its two tabs, the back arrow on a post or a profile. Twister’s titlebar already says where you are. Headers that carry tabs or a search field elsewhere stay; the new-posts pill on Home stays. Turn on Following first too, or Home stays on For you.',
  },
  {
    key: 'hideTimelineModules',
    label: 'Only posts in the timeline',
    hint: 'No “Who to follow”, “Discover more”, news or premium blocks between posts on Home, profiles, lists and bookmarks. Under a post, “Discover more” and everything after it goes.',
  },
  {
    key: 'timeOnRight',
    label: 'Time on the right',
    hint: 'The post’s age sits at the far right of the byline, the dot before it gone. The post on its own page keeps X’s layout.',
  },
  {
    key: 'mediaThumbnails',
    label: 'Media as thumbnails',
    hint: 'Photos and videos in a timeline as a 180px strip, cropped to fit, the way a classic client showed them. The post on its own page keeps them full size.',
  },
]

/** The OS's own face — San Francisco on macOS, Segoe on Windows — as CSS names it. */
const SYSTEM_FONT = 'system-ui'

const SHORTCUTS: Array<{ keys: string; does: string }> = [
  {
    keys: `${MOD_KEY}1 – ${MOD_KEY}6`,
    does: 'Home, Explore, Notifications, Messages, Bookmarks, Profile',
  },
  { keys: `${MOD_KEY}N`, does: 'New post' },
  { keys: `${MOD_KEY}T  ${MOD_KEY}W`, does: 'New tab, close tab' },
  { keys: 'Ctrl⇥  Ctrl⇧⇥', does: 'Next tab, previous tab' },
  { keys: `${MOD_KEY}[  ${MOD_KEY}]`, does: 'Back, forward' },
  { keys: `${MOD_KEY}R`, does: 'Reload the page' },
  { keys: `${MOD_KEY}⇧P  ${MOD_KEY}⇧O  ${MOD_KEY}⇧N`, does: 'People, Posts, Write' },
  { keys: `${MOD_KEY}\\`, does: 'Collapse or expand the sidebar' },
  { keys: `${MOD_KEY},`, does: 'Settings' },
]

function SettingsScreen() {
  const settings = useSettings()
  const update = useUpdateSettings()
  const [error, setError] = React.useState<string | null>(null)

  const patch = React.useCallback(
    (change: Partial<Settings>) => {
      if (!settings.data) return
      const next = { ...settings.data, ...change }
      void (async () => {
        try {
          await update.mutateAsync(next)
          setError(null)
        } catch (err) {
          setError(humanMessage(err))
        }
      })()
    },
    [settings.data, update],
  )

  const current = settings.data
  if (!current) return null

  return (
    <div className="mx-auto flex max-w-2xl flex-col gap-8 p-4 pb-8">
      {error ? (
        <p className="rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive">
          {error}
        </p>
      ) : null}

      <Section title="Appearance">
        <Row label="Theme" hint="System follows your desktop. X keeps its own theme either way.">
          <Select
            value={current.theme}
            onChange={(event) => {
              patch({ theme: event.target.value as Settings['theme'] })
            }}
            className="w-36"
          >
            <option value="system">System</option>
            <option value="light">Light</option>
            <option value="dark">Dark</option>
          </Select>
        </Row>
        <Row
          label="Window material"
          hint="Frosts the window chrome. macOS and Windows only — Linux compositors mostly refuse, and Twister falls back to solid."
        >
          <Select
            value={current.windowMaterial}
            onChange={(event) => {
              patch({ windowMaterial: event.target.value as Settings['windowMaterial'] })
            }}
            className="w-36"
          >
            <option value="off">Off</option>
            <option value="standard">Standard</option>
            <option value="strong">Strong</option>
          </Select>
        </Row>
      </Section>

      <Section
        title="The look"
        note="Posts and the composer the way a classic client drew them. Selectors on X’s page, like the niceties: one that stops matching does nothing until it is fixed."
      >
        <Row
          label="Font on X"
          hint="A font family for X’s text, as CSS would name it — “Inter”, “Georgia, serif”. Empty keeps X’s own."
        >
          <div className="flex items-center gap-1.5">
            <Button
              size="sm"
              variant={current.font === SYSTEM_FONT ? 'default' : 'outline'}
              onClick={() => {
                patch({ font: current.font === SYSTEM_FONT ? '' : SYSTEM_FONT })
              }}
            >
              System
            </Button>
            <FontField
              // Remounted when the stored value changes, so the draft starts from it.
              key={current.font}
              value={current.font}
              onCommit={(font) => {
                patch({ font })
              }}
            />
          </div>
        </Row>
        <Row label="Text size" hint="The size of a post’s text. Normal is X’s own.">
          <Select
            value={current.textSize}
            onChange={(event) => {
              patch({ textSize: event.target.value as Settings['textSize'] })
            }}
            className="w-36"
          >
            <option value="small">Small</option>
            <option value="normal">Normal</option>
            <option value="large">Large</option>
          </Select>
        </Row>
        {LOOK.map((item) => (
          <Row key={item.key} label={item.label} hint={item.hint}>
            <Switch
              checked={current.niceties[item.key]}
              onCheckedChange={(checked) => {
                patch({ niceties: { ...current.niceties, [item.key]: checked } })
              }}
            />
          </Row>
        ))}
      </Section>

      <Section
        title="Niceties"
        note="What Twister changes about X. Each one is a selector on X’s own page, which X changes without notice — one that stops working can be switched off here until the next update."
      >
        {NICETIES.map((nicety) => (
          <Row key={nicety.key} label={nicety.label} hint={nicety.hint}>
            <Switch
              checked={current.niceties[nicety.key]}
              onCheckedChange={(checked) => {
                patch({ niceties: { ...current.niceties, [nicety.key]: checked } })
              }}
            />
          </Row>
        ))}
      </Section>

      <UserlandSection />

      <StoreSection />

      <AccountSection />

      <Section title="Shortcuts" note="These work whichever part of the window has focus.">
        {SHORTCUTS.map((shortcut) => (
          <Row key={shortcut.keys} label={shortcut.does}>
            <Kbd>{shortcut.keys}</Kbd>
          </Row>
        ))}
      </Section>

      <ApiCostsSection />

      <AboutSection />
    </div>
  )
}

/**
 * The bill Twister does not run up: what X's API charges for each thing a classic client did, and
 * what a day of it comes to. A reference, from X's own card, so "the API costs money" is a number.
 */
function ApiCostsSection() {
  const day = dayCost()
  const checked = new Date(`${RATE_CARD_CHECKED}T00:00:00`).toLocaleDateString(undefined, {
    year: 'numeric',
    month: 'long',
    day: 'numeric',
  })
  const cut = new Date(`${SELF_SERVE_CUT}T00:00:00`).toLocaleDateString(undefined, {
    year: 'numeric',
    month: 'long',
    day: 'numeric',
  })
  return (
    <Section
      title="What the API would charge"
      note="Twister makes no API calls: the island is X's own site. This is what X's pay-per-use card charges for each thing a classic client did, and what an ordinary day of it comes to."
    >
      <div className="overflow-x-auto">
        <table className="w-full text-xs">
          <thead>
            <tr className="text-left text-[11px] text-muted-foreground">
              <th className="px-3 py-2 font-medium">You</th>
              <th className="px-3 py-2 font-medium">X’s card</th>
              <th className="px-3 py-2 text-right font-medium">A day</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-border/50">
            {CLIENT_ACTIONS.map((action) => (
              <tr key={action.does}>
                <td className="px-3 py-1.5">{action.does}</td>
                <td className="px-3 py-1.5 text-muted-foreground">
                  {action.row}
                  {action.usd !== null && action.unit ? (
                    <span className="tabular-nums">
                      {' '}
                      · {usd(action.usd)} a {action.unit}
                    </span>
                  ) : null}
                </td>
                <td className="px-3 py-1.5 text-right whitespace-nowrap tabular-nums">
                  {action.usd === null ? (
                    <span className="text-muted-foreground">
                      {action.perDay} {action.each} · —
                    </span>
                  ) : (
                    <>
                      <span className="text-muted-foreground">
                        {action.perDay} {action.each} ·{' '}
                      </span>
                      {usd(action.usd * action.perDay)}
                    </>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <div className="flex items-baseline justify-between gap-3 px-3 py-2.5 text-sm">
        <span>
          {usd(day.usd)} a day, about {usd(day.usd * 30)} a month, before{' '}
          {day.unpriced.map((name) => name.toLowerCase()).join(', ')}.
        </span>
        <span className="shrink-0 font-medium text-success">Twister: $0</span>
      </div>
      <p className="px-3 py-2 text-[11px] leading-relaxed text-muted-foreground">
        Read off docs.x.com on {checked}; the card has moved twice this year. Reads bill per
        resource returned and are charged once per UTC day, so the counts are distinct posts and
        people rather than scrolls. Likes, follows and quote posts left every self-serve tier on{' '}
        {cut}; reposts and deletes have no row of their own. The card caps a month at three million
        post reads.
      </p>
    </Section>
  )
}

/**
 * The session is X's, held in the site view's own cookie store; Twister never reads it. Signing out
 * clears that store, which is the only thing Twister can do to it.
 */
function AccountSection() {
  const site = useSiteState()
  const signOut = useSignOut()
  const [confirming, setConfirming] = React.useState(false)
  const [error, setError] = React.useState<string | null>(null)

  return (
    <Section title="Account">
      <Row
        label={site.data?.handle ? `@${site.data.handle}` : 'Not signed in yet'}
        hint={
          site.data?.handle
            ? 'Signed in on X. Your session lives in the site view, not in Twister.'
            : 'X shows its own sign-in form in the island. Twister sees the handle once X draws its navigation.'
        }
      >
        {confirming ? (
          <div className="flex items-center gap-1.5">
            <Button
              size="sm"
              variant="destructive"
              onClick={() => {
                void (async () => {
                  try {
                    await signOut.mutateAsync()
                    setConfirming(false)
                    setError(null)
                  } catch (err) {
                    setError(humanMessage(err))
                  }
                })()
              }}
            >
              Sign out
            </Button>
            <Button
              size="sm"
              variant="ghost"
              onClick={() => {
                setConfirming(false)
              }}
            >
              Keep
            </Button>
          </div>
        ) : (
          <Button
            size="sm"
            variant="outline"
            onClick={() => {
              setConfirming(true)
            }}
          >
            Sign out…
          </Button>
        )}
      </Row>
      {confirming ? (
        <p className="px-3 py-2 text-xs leading-relaxed text-muted-foreground">
          Clears every cookie and all site data X has stored in this app, then returns to X’s front
          door. Your account itself is untouched.
        </p>
      ) : null}
      {error ? <p className="px-3 py-2 text-xs text-destructive">{error}</p> : null}
    </Section>
  )
}

/**
 * The user's own scripts and styles: the Tampermonkey and Stylus of this client. Files in two
 * folders, read when the site view is built, so a change needs a reload — which rebuilds the view.
 */
function UserlandSection() {
  const assets = useUserAssets()
  const openDir = useOpenUserAssetsDir()
  const reload = useReloadSite()
  const [error, setError] = React.useState<string | null>(null)

  const run = (mutation: { mutateAsync: () => Promise<unknown> }) => {
    void (async () => {
      try {
        await mutation.mutateAsync()
        setError(null)
      } catch (err) {
        setError(humanMessage(err))
      }
    })()
  }

  const scripts = assets.data?.scripts ?? []
  const styles = assets.data?.styles ?? []

  return (
    <Section
      title="Scripts and styles"
      note="Your own userscripts and userstyles, injected into X on every page. Drop *.js into the scripts folder and *.css into the styles folder; scripts run once the page is ready, like Tampermonkey's default, minus the GM_* API. Files load in name order."
    >
      <Row
        label={
          scripts.length + styles.length === 0
            ? 'Nothing in the folders yet'
            : `${scripts.length} script${scripts.length === 1 ? '' : 's'}, ${styles.length} style${styles.length === 1 ? '' : 's'}`
        }
        hint={assets.data?.dir}
      >
        <div className="flex items-center gap-1.5">
          <Button
            size="sm"
            variant="outline"
            onClick={() => {
              run(openDir)
            }}
          >
            Open folder
          </Button>
          <Button
            size="sm"
            variant="outline"
            onClick={() => {
              run(reload)
            }}
          >
            Reload
          </Button>
        </div>
      </Row>
      {[...scripts, ...styles].map((asset) => (
        <div
          key={asset.name}
          className="flex items-center gap-3 px-3 py-2 font-mono text-xs text-muted-foreground"
        >
          <span className="truncate text-foreground">{asset.name}</span>
          <span className="ml-auto shrink-0 tabular-nums">{formatBytes(asset.bytes)}</span>
        </div>
      ))}
      {error ? <p className="px-3 py-2 text-xs text-destructive">{error}</p> : null}
    </Section>
  )
}

/** Committed on blur or Enter rather than per keystroke: each commit rebuilds X's styling. */
function FontField({ value, onCommit }: { value: string; onCommit: (font: string) => void }) {
  const [draft, setDraft] = React.useState(value)
  return (
    <Input
      value={draft}
      placeholder="X’s own"
      className="w-48"
      onChange={(event) => {
        setDraft(event.target.value)
      }}
      onBlur={() => {
        if (draft.trim() !== value) onCommit(draft.trim())
      }}
      onKeyDown={(event) => {
        if (event.key === 'Enter') event.currentTarget.blur()
      }}
    />
  )
}

/**
 * The store: what the capture hook has kept, and the way to forget it. It holds other people's
 * profiles and posts, so clearing it is one click and signing out offers it too.
 */
function StoreSection() {
  const counts = useStoreCounts()
  const clear = useClearCaptured()
  const openDownloads = useOpenDownloadsDir()
  const [confirming, setConfirming] = React.useState(false)
  const [error, setError] = React.useState<string | null>(null)

  const run = (action: () => Promise<unknown>) => {
    void (async () => {
      try {
        await action()
        setError(null)
      } catch (err) {
        setError(humanMessage(err))
      }
    })()
  }

  const people = counts.data?.users ?? 0
  const posts = counts.data?.posts ?? 0

  return (
    <Section
      title="The store"
      note="What the People and Posts tools work from: the people and posts X loaded into a tab while Twister watched. One SQLite file in the app data directory, and the agent door (twister-mcp) reads the same file."
    >
      <Row
        label={`${people.toLocaleString()} people, ${posts.toLocaleString()} posts`}
        hint="Grows as you browse and as you scan pages from the tools."
      >
        {confirming ? (
          <div className="flex items-center gap-1.5">
            <Button
              size="sm"
              variant="destructive"
              onClick={() => {
                run(async () => clear.mutateAsync())
                setConfirming(false)
              }}
            >
              Forget everything
            </Button>
            <Button
              size="sm"
              variant="ghost"
              onClick={() => {
                setConfirming(false)
              }}
            >
              Keep
            </Button>
          </div>
        ) : (
          <Button
            size="sm"
            variant="outline"
            disabled={people + posts === 0}
            onClick={() => {
              setConfirming(true)
            }}
          >
            Clear…
          </Button>
        )}
      </Row>
      <Row label="Downloads" hint="Where the download button saves media: Downloads/Twister.">
        <Button
          size="sm"
          variant="outline"
          onClick={() => {
            run(async () => openDownloads.mutateAsync())
          }}
        >
          Open folder
        </Button>
      </Row>
      {error ? <p className="px-3 py-2 text-xs text-destructive">{error}</p> : null}
    </Section>
  )
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  return `${(bytes / 1024).toFixed(bytes >= 10_240 ? 0 : 1)} KB`
}

function AboutSection() {
  const [version, setVersion] = React.useState<string>('')
  React.useEffect(() => {
    void getVersion()
      .then(setVersion)
      .catch(() => {})
  }, [])

  return (
    <Section title="About">
      <Row label={version ? `Twister ${version}` : 'Twister'}>
        <span className="text-xs text-muted-foreground">Tauri 2 · React 19</span>
      </Row>
      <p className="px-3 py-2.5 text-xs leading-relaxed text-muted-foreground">
        A desktop client for X with niceties injected. The page in the island is X’s own site, in
        its own view, with its own session; Twister frames it, keeps the shortcuts, opens links
        outside in your browser, and hides what you ask it to. Nothing you read or write passes
        through Twister.
      </p>
    </Section>
  )
}

function Section({
  title,
  note,
  children,
}: {
  title: string
  note?: string
  children: React.ReactNode
}) {
  return (
    <section>
      <h2 className="mb-2 font-display text-xs font-semibold tracking-wide text-muted-foreground uppercase">
        {title}
      </h2>
      {note ? <p className="mb-3 text-xs leading-relaxed text-muted-foreground">{note}</p> : null}
      <div className="divide-y divide-border/50 rounded-lg border border-border/60 bg-card/60">
        {children}
      </div>
    </section>
  )
}

function Row({
  label,
  hint,
  children,
}: {
  label: string
  hint?: string
  children: React.ReactNode
}) {
  return (
    <div className="flex items-center gap-4 px-3 py-2.5">
      <div className="min-w-0 flex-1">
        <div className="text-sm">{label}</div>
        {hint ? <div className="text-xs leading-relaxed text-muted-foreground">{hint}</div> : null}
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  )
}
