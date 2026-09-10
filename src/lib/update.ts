/**
 * What the update surfaces say, and how far along a download is. Pure functions only — Settings,
 * the status bar and the menu's "Check for Updates…" all read them, so none of them can disagree
 * about the same download or the same failure.
 */

import { errorCode, humanMessage } from '@/lib/tauri/client'
import type { UpdateProgress } from '@/lib/tauri/types'

/** How an update failure reads, and whether trying again could possibly help. */
export interface UpdateFailure {
  title: string
  detail: string
  /** False where a retry is useless or unsafe — a bundle that failed its signature check. */
  retryable: boolean
  /**
   * The calm pre-first-release state rather than a failure: this channel simply has nothing
   * published yet. Drawn as a note, not an alarm.
   */
  calm: boolean
}

/**
 * Classify an update failure. The `[CODE]` prefix Rust stamps on every error (see
 * `src-tauri/src/error.rs`) settles the two cases only Rust can tell apart — a channel with no
 * release versus a releases repository that is gone. Everything else the updater plugin reports is
 * a string, so the rest is substring work on the message.
 */
export function describeUpdateFailure(error: unknown): UpdateFailure {
  const message = humanMessage(error)
  const lower = message.toLowerCase()

  const code = errorCode(error)
  if (code === 'NO_RELEASE') {
    return {
      title: 'Nothing published yet',
      detail: `${message} Twister will find it when there is.`,
      retryable: true,
      calm: true,
    }
  }
  if (code === 'UPDATE_SOURCE_UNREACHABLE') {
    return {
      title: 'The releases page did not answer',
      detail: `${message} Either this machine is offline or the update pipeline is broken — this is not the same as there being no new version.`,
      retryable: true,
      calm: false,
    }
  }

  if (lower.includes('signature') || lower.includes('minisign')) {
    return {
      title: 'That download could not be trusted',
      detail:
        'The bundle did not match its signature, so Twister refused it. Nothing was installed. Download the new version from the releases page instead.',
      retryable: false,
      calm: false,
    }
  }
  if (/error sending request|connection|timed ?out|timeout|dns|network/.test(lower)) {
    return {
      title: 'Could not reach the releases page',
      detail: 'The network did not answer. Try again once it does.',
      retryable: true,
      calm: false,
    }
  }
  if (lower.includes('no space') || lower.includes('disk')) {
    return {
      title: 'Not enough room',
      detail: 'The update could not be written to disk. Free some space and try again.',
      retryable: true,
      calm: false,
    }
  }
  return { title: 'The update failed', detail: message, retryable: true, calm: false }
}

/** A byte count as a human reads it ("42.3 MB"). Nothing in, nothing out. */
export function formatUpdateSize(bytes: number | null | undefined): string | null {
  if (bytes === null || bytes === undefined || !Number.isFinite(bytes) || bytes < 0) return null
  const mb = bytes / (1024 * 1024)
  return mb >= 1024 ? `${(mb / 1024).toFixed(2)} GB` : `${mb.toFixed(1)} MB`
}

/**
 * How far along, as a percentage — or null when there is nothing to show: no download in flight, or
 * a release server that sent no `Content-Length`, which the bar draws as indeterminate rather than
 * as frozen at zero.
 */
export function updateProgressPercent(progress: UpdateProgress | undefined): number | null {
  if (!progress || progress.total === null || progress.total <= 0) return null
  return Math.min(100, Math.round((progress.downloaded / progress.total) * 100))
}
