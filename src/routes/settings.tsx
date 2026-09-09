import { createFileRoute } from '@tanstack/react-router'
import { getVersion } from '@tauri-apps/api/app'
import * as React from 'react'

import { Button } from '@/components/ui/button'
import { Kbd } from '@/components/ui/kbd'
import { Select } from '@/components/ui/select'
import { Switch } from '@/components/ui/switch'
import { MOD_KEY } from '@/lib/chrome'
import {
  useOpenUserAssetsDir,
  useReloadSite,
  useSettings,
  useSignOut,
  useSiteState,
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
    key: 'classicBird',
    label: 'The bird',
    hint: 'The blue bird in place of the X mark, and the classic blue on Post and Follow.',
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

const SHORTCUTS: Array<{ keys: string; does: string }> = [
  {
    keys: `${MOD_KEY}1 – ${MOD_KEY}6`,
    does: 'Home, Explore, Notifications, Messages, Bookmarks, Profile',
  },
  { keys: `${MOD_KEY}N`, does: 'New post' },
  { keys: `${MOD_KEY}[  ${MOD_KEY}]`, does: 'Back, forward' },
  { keys: `${MOD_KEY}R`, does: 'Reload the page' },
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

      <AccountSection />

      <Section title="Shortcuts" note="These work whichever part of the window has focus.">
        {SHORTCUTS.map((shortcut) => (
          <Row key={shortcut.keys} label={shortcut.does}>
            <Kbd>{shortcut.keys}</Kbd>
          </Row>
        ))}
      </Section>

      <AboutSection />
    </div>
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
