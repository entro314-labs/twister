// Twister's operations on Threads: a scan, and nothing else. Injected at
// document start into every Threads tab, after common.js, which holds the
// runner; started by Rust (`ops.rs`) in the active tab.
//
// Meta's sites are watched, not driven. Rust refuses a follow, unfollow,
// delete or compose before it is written down (`Network::supports`), and
// the runner refuses them again here for anything that gets past that.
// Posts are pressable containers; a person's link starts with /@.
(() => {
  'use strict'
  if (window.__twisterOps) return
  const common = window.__twisterCommon
  if (!common) return

  common.createOps({
    siteName: 'Threads',
    userCell: 'a[href^="/@"]',
    article: '[data-pressable-container="true"]',
    column: null,
    empty: null,
    toast: null,
    lockout: /^\/login(?:[/?#]|$)/,
    limits: { follow: 0, unfollow: 0, delete: 0 },
    ownHandle: () => '',
    handleOf: () => '',
    postOf: () => '',
  })
})()
