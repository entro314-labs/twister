/**
 * The IPC registry. Every command and event name lives here rather than as a string literal at the
 * call site, so a rename in Rust breaks the build in one place instead of failing silently at
 * runtime. The three `site_*` commands are the bridge's and are not callable from here.
 */
export const IPC_COMMANDS = {
  getSettings: 'get_settings',
  updateSettings: 'update_settings',
  getSiteState: 'get_site_state',
  navigateSite: 'navigate_site',
  siteAction: 'site_action',
  setSiteInsets: 'set_site_insets',
  setSiteVisible: 'set_site_visible',
  signOut: 'sign_out',
  shellReady: 'shell_ready',
  setWindowMaterial: 'set_window_material',
  listUserAssets: 'list_user_assets',
  openUserAssetsDir: 'open_user_assets_dir',
  reloadSite: 'reload_site',
  showTooltip: 'show_tooltip',
  hideTooltip: 'hide_tooltip',
} as const

export const IPC_EVENTS = {
  /** The whole site state, whenever any of it changes. Payload: `SiteState`. */
  siteState: 'twister://site-state',
  /** A one-line notice for the status bar. Payload: `Notice`. */
  notice: 'twister://notice',
  /** The menu asked the shell for something. Payload: `ShellAction`. */
  shell: 'twister://shell',
} as const

type ValueOf<T> = T[keyof T]

export type IpcCommand = ValueOf<typeof IPC_COMMANDS>
export type IpcEvent = ValueOf<typeof IPC_EVENTS>
