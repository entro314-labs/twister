#!/usr/bin/env node
/* oxlint-disable no-console -- this is a CLI tool; console is its output channel */

/**
 * Version manager for Twister.
 *
 * Bumps the app version coherently across every file a release touches, so a local `pnpm
 * version:*` produces exactly what release-kit would write from `release.config.json`. The CURRENT
 * version is read from the root `package.json` — the single source of truth — and every other file
 * is rewritten only where it still carries that same version, so a file that has drifted is
 * reported as `skipped` rather than silently overwritten with a value that was never true.
 *
 * Run with plain `node` — Node 26 strips the types itself.
 */

import { existsSync, readFileSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const projectRoot = join(dirname(fileURLToPath(import.meta.url)), '../..')

type VersionFileType = 'json' | 'toml' | 'cargo-lock'
type VersionPart = 'major' | 'minor' | 'patch'

interface VersionFile {
  path: string
  type: VersionFileType
  /** JSON/TOML: the version key. `cargo-lock`: the `[[package]]` name whose version to bump. */
  key: string
}

const ROOT_PACKAGE_JSON = join(projectRoot, 'package.json')
const tauri = (file: string): string => join(projectRoot, 'src-tauri', file)

// The same target set `release.config.json` names, kept in lockstep so a local bump and a release
// write the same four files. `tauri.conf.json` is the one the running app reports as its version.
const VERSION_FILES: VersionFile[] = [
  { path: ROOT_PACKAGE_JSON, type: 'json', key: 'version' },
  { path: tauri('tauri.conf.json'), type: 'json', key: 'version' },
  { path: tauri('Cargo.toml'), type: 'toml', key: 'version' },
  { path: tauri('Cargo.lock'), type: 'cargo-lock', key: 'twister' },
]

const SEMVER_RE = /^(\d+)\.(\d+)\.(\d+)$/
// Full semver incl. prerelease — accepted by `set` and when READING the current version, because a
// channel release leaves e.g. 0.2.0-alpha.1 in the files.
const FULL_SEMVER_RE = /^\d+\.\d+\.\d+(-[A-Za-z0-9.-]+)?$/

/** The current version, from the root `package.json`. */
function readCurrentVersion(): string {
  const pkg = JSON.parse(readFileSync(ROOT_PACKAGE_JSON, 'utf8')) as { version?: unknown }
  const version = pkg.version
  if (typeof version !== 'string' || !FULL_SEMVER_RE.test(version)) {
    throw new Error(`package.json version is not semver: ${String(version)}`)
  }
  return version
}

/** Increment one semver part, zeroing the lower parts. */
function bumpVersion(current: string, part: VersionPart): string {
  const match = SEMVER_RE.exec(current)
  if (!match) {
    throw new Error(
      `Cannot increment a prerelease version (${current}); use \`set X.Y.Z[-suffix]\` instead`,
    )
  }
  let major = Number(match[1])
  let minor = Number(match[2])
  let patch = Number(match[3])
  if (part === 'major') {
    major += 1
    minor = 0
    patch = 0
  } else if (part === 'minor') {
    minor += 1
    patch = 0
  } else {
    patch += 1
  }
  return `${major}.${minor}.${patch}`
}

type UpdateOutcome = 'updated' | 'skipped' | 'missing'

const escapeRegExp = (value: string): string => value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')

/**
 * Rewrite a top-level JSON `version` key, but only where it still equals `oldVersion` — a file that
 * has drifted is reported rather than quietly assigned a version it never had.
 */
function updateJsonVersion(
  filePath: string,
  key: string,
  oldVersion: string,
  newVersion: string,
): UpdateOutcome {
  if (!existsSync(filePath)) return 'missing'
  const text = readFileSync(filePath, 'utf8')
  const endsWithNewline = text.endsWith('\n')
  const content = JSON.parse(text) as Record<string, unknown>
  if (content[key] !== oldVersion) return 'skipped'
  content[key] = newVersion
  writeFileSync(filePath, JSON.stringify(content, null, 2) + (endsWithNewline ? '\n' : ''))
  return 'updated'
}

/** Rewrite the first `version = "<old>"` line of a TOML file — the Cargo package version. */
function updateTomlVersion(
  filePath: string,
  key: string,
  oldVersion: string,
  newVersion: string,
): UpdateOutcome {
  if (!existsSync(filePath)) return 'missing'
  const text = readFileSync(filePath, 'utf8')
  const re = new RegExp(`^${escapeRegExp(key)}\\s*=\\s*"${escapeRegExp(oldVersion)}"`, 'm')
  if (!re.test(text)) return 'skipped'
  writeFileSync(filePath, text.replace(re, `${key} = "${newVersion}"`))
  return 'updated'
}

/** Rewrite the version inside the `[[package]] name = "<pkg>"` block of a Cargo.lock. */
function updateCargoLockVersion(
  filePath: string,
  pkgName: string,
  newVersion: string,
): UpdateOutcome {
  if (!existsSync(filePath)) return 'missing'
  const text = readFileSync(filePath, 'utf8')
  const re = new RegExp(
    `(\\[\\[package\\]\\]\\nname = "${escapeRegExp(pkgName)}"\\nversion = ")[^"]+(")`,
  )
  if (!re.test(text)) return 'skipped'
  writeFileSync(filePath, text.replace(re, `$1${newVersion}$2`))
  return 'updated'
}

function updateFile(file: VersionFile, oldVersion: string, newVersion: string): UpdateOutcome {
  switch (file.type) {
    case 'json':
      return updateJsonVersion(file.path, file.key, oldVersion, newVersion)
    case 'toml':
      return updateTomlVersion(file.path, file.key, oldVersion, newVersion)
    case 'cargo-lock':
      return updateCargoLockVersion(file.path, file.key, newVersion)
  }
}

function write(from: string, to: string, dryRun: boolean): void {
  if (dryRun) return
  for (const file of VERSION_FILES) {
    console.log(`${updateFile(file, from, to).padEnd(8)} ${file.path}`)
  }
}

/**
 * Set an explicit version (prerelease allowed) across every file. Channel releases REQUIRE this:
 * the tag's version has to match the version baked into the binaries, or the updater compares the
 * wrong two numbers.
 */
export function setVersion(explicit: string, dryRun = false): { from: string; to: string } {
  if (!FULL_SEMVER_RE.test(explicit)) throw new Error(`Not a semver version: ${explicit}`)
  const from = readCurrentVersion()
  write(from, explicit, dryRun)
  return { from, to: explicit }
}

/** Bump one part of the version across every file. */
export function incrementVersion(part: VersionPart = 'patch', dryRun = false) {
  const from = readCurrentVersion()
  const to = bumpVersion(from, part)
  write(from, to, dryRun)
  return { from, to }
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const args = process.argv.slice(2)
  const command = args[0] ?? 'increment'
  const part: VersionPart = args.includes('--major')
    ? 'major'
    : args.includes('--minor')
      ? 'minor'
      : 'patch'
  const dryRun = args.includes('--dry-run')
  const prefix = dryRun ? '[dry-run] ' : ''

  switch (command) {
    case 'increment': {
      const result = incrementVersion(part, dryRun)
      console.log(`${prefix}${result.from} -> ${result.to}`)
      break
    }
    case 'set': {
      const explicit = args[1]
      if (!explicit) {
        console.error('Usage: version-manager.ts set X.Y.Z[-suffix]')
        process.exit(1)
      }
      const result = setVersion(explicit, dryRun)
      console.log(`${prefix}${result.from} -> ${result.to}`)
      break
    }
    case 'current':
      console.log(readCurrentVersion())
      break
    case 'help':
      console.log(
        'Usage: version-manager.ts [increment|set X.Y.Z[-suffix]|current] [--major|--minor|--patch] [--dry-run]',
      )
      break
    default:
      console.error(`Unknown command: ${command}`)
      process.exit(1)
  }
}
