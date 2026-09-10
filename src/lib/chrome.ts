/**
 * Window-chrome constants. Shared because the sidebar header, the pane titlebar and the macOS
 * traffic lights all have to agree on one band height — a mismatch of a couple of pixels is
 * instantly visible as a stepped seam. Rust's default insets in `site.rs` mirror these.
 */
export const APP_NAME = 'Twister'

/**
 * The titlebar band. 52 is the macOS toolbar-window title-bar height, which is what
 * `windowing::apply_macos_chrome` gives the window (an empty unified `NSToolbar`, so AppKit places
 * the lights itself). The band starts at y = 0, so its centre is 26 — exactly the line AppKit
 * centres the traffic lights on. Off macOS nothing keys off it; one height everywhere keeps the
 * sidebar header, the pane titlebar and our own window controls on one baseline.
 */
export const TITLEBAR_H = 52
/**
 * Width the macOS traffic lights occupy from the window's leading edge. AppKit places them with no
 * help from us: 14px buttons on a 23px pitch starting at x = 19, so the green button's right edge
 * lands at 79. (macOS 14/15 draws 12px buttons ending ~72 — the clearances below simply run a few
 * px looser there; sizing to the current, larger metric is what keeps the green light clear of the
 * rail boundary either way.)
 */
const TRAFFIC_LIGHTS_W = 80
/**
 * Leading inset for content at the left edge of the titlebar on macOS. A full 16px gap after the
 * green button rather than butting the wordmark against it.
 */
export const TITLEBAR_INSET_LEFT = TRAFFIC_LIGHTS_W + 16
/** Fullscreen hides the lights, so only an ordinary gutter is left to clear. */
export const TITLEBAR_INSET_LEFT_FULLSCREEN = 14

export const SIDEBAR_DEFAULT_W = 236
export const SIDEBAR_MIN_W = 190
export const SIDEBAR_MAX_W = 340
/** MacOS draws its own traffic lights; the other two need ours. */
export const IS_MACOS =
  typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.platform ?? '')

/**
 * Wide enough on macOS for the three traffic lights to sit entirely inside it (they end at
 * `TRAFFIC_LIGHTS_W`), so the rail's toggle and its icons line up beneath them on one axis instead
 * of straddling the boundary with the island's titlebar.
 */
export const SIDEBAR_RAIL_W = IS_MACOS ? TRAFFIC_LIGHTS_W + 4 : 56

/** The tool panels (People, Posts, Write) sit beside the island at this width. */
export const TOOLS_PANEL_W = 400

/** The modifier glyph for shortcut hints. */
export const MOD_KEY = IS_MACOS ? '⌘' : 'Ctrl'
