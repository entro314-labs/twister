/**
 * The IPC registry. Every command and event name lives here rather than as a string literal at the
 * call site, so a rename in Rust breaks the build in one place instead of failing silently at
 * runtime. The `site_*` commands are the bridge's and are not callable from here.
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
  newTab: 'new_tab',
  closeTab: 'close_tab',
  activateTab: 'activate_tab',
  getStoreCounts: 'get_store_counts',
  listPeople: 'list_people',
  listPosts: 'list_posts',
  exportPeople: 'export_people',
  exportPosts: 'export_posts',
  clearCaptured: 'clear_captured',
  startOp: 'start_op',
  cancelOp: 'cancel_op',
  getOps: 'get_ops',
  preparePost: 'prepare_post',
  postNow: 'post_now',
  schedulePost: 'schedule_post',
  listScheduledPosts: 'list_scheduled_posts',
  deleteScheduledPost: 'delete_scheduled_post',
  openDownloadsDir: 'open_downloads_dir',
  updateState: 'update_state',
  checkForUpdate: 'check_for_update',
  stageUpdate: 'stage_update',
  restartAndInstall: 'restart_and_install',
} as const

export const IPC_EVENTS = {
  /** The whole site state, whenever any of it changes. Payload: `SiteState`. */
  siteState: 'twister://site-state',
  /** A one-line notice for the status bar. Payload: `Notice`. */
  notice: 'twister://notice',
  /** The menu asked the shell for something. Payload: `ShellAction`. */
  shell: 'twister://shell',
  /** The running operation changed. Payload: `Job`. */
  op: 'twister://op',
  /** The schedule changed. No payload. */
  schedule: 'twister://schedule',
  /** An update is downloading. Payload: `UpdateProgress`. */
  updateProgress: 'twister://update-progress',
} as const

type ValueOf<T> = T[keyof T]

export type IpcCommand = ValueOf<typeof IPC_COMMANDS>
export type IpcEvent = ValueOf<typeof IPC_EVENTS>
