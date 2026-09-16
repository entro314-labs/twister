// Twister's operations on Instagram: a scan, and nothing else. Injected at
// document start into every Instagram tab, after common.js, which holds
// the runner; started by Rust (`ops.rs`) in the active tab.
//
// Meta's sites are watched, not driven. Rust refuses a follow, unfollow,
// delete or compose before it is written down (`Network::supports`), and
// the runner refuses them again here for anything that gets past that.
// Posts are <article>s; a person's link is a bare /handle/.
(() => {
  'use strict'
  if (window.__twisterOps) return
  const common = window.__twisterCommon
  if (!common) return

  common.createOps({
    siteName: 'Instagram',
    userCell: 'main a[href^="/"]',
    article: 'article',
    column: 'main',
    empty: null,
    toast: null,
    lockout: /^\/accounts\/(?:login|suspended|challenge)/,
    limits: { follow: 0, unfollow: 0, delete: 0 },
    ownHandle: () => '',
    handleOf: () => '',
    postOf: () => '',
  })
})()
