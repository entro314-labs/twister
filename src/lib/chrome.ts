/**
 * Window-chrome constants. Shared because the sidebar header, the pane titlebar and the macOS
 * traffic lights all have to agree on one band height — a mismatch of a couple of pixels is
 * instantly visible as a stepped seam. Rust's default insets in `site.rs` mirror these.
 */
export const APP_NAME = 'Twister'

export const TITLEBAR_H = 44
/**
 * Clears the macOS traffic lights, whose position is set in `lib.rs` (x: 18). The three buttons end
 * around 77px; this leaves a real gap after them rather than butting the wordmark against the green
 * one.
 */
export const TITLEBAR_INSET_LEFT = 94

export const SIDEBAR_DEFAULT_W = 236
export const SIDEBAR_MIN_W = 190
export const SIDEBAR_MAX_W = 340
/** MacOS draws its own traffic lights; the other two need ours. */
export const IS_MACOS =
  typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.platform ?? '')

/**
 * Wide enough on macOS for the three traffic lights (which end near 77px) to sit inside it, so the
 * rail's toggle and its icons line up beneath them on one axis instead of colliding with the
 * island's titlebar.
 */
export const SIDEBAR_RAIL_W = IS_MACOS ? 84 : 56

/** The modifier glyph for shortcut hints. */
export const MOD_KEY = IS_MACOS ? '⌘' : 'Ctrl'
