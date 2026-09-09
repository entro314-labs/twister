// Twister's bridge into x.com. Injected at document start into the site
// webview, main frame only, on every page load.
//
// Everything that reaches into X's DOM lives here and in niceties.css: X ships
// changes without notice, and a broken nicety should be a one-file fix. Rust
// substitutes the two __TWISTER_*__ placeholders before injection.
//
// The bridge can call exactly three commands — site_settings, site_navigated
// and site_profile — and each validates what it is sent, because anything
// running on this page could call them too.
(() => {
  'use strict'
  if (window.__twister) return

  const internals = window.__TAURI_INTERNALS__
  const invoke = (command, args) =>
    internals ? internals.invoke(command, args).catch(() => undefined) : Promise.resolve(undefined)

  const CSS = __TWISTER_CSS__
  let niceties = __TWISTER_NICETIES__

  const ATTRS = {
    hidePromoted: 'data-twister-hide-promoted',
    hideRightColumn: 'data-twister-hide-right-column',
    hideExtrasNav: 'data-twister-hide-extras-nav',
    hideViewCounts: 'data-twister-hide-view-counts',
    hideSiteNav: 'data-twister-hide-site-nav',
  }

  function installStyles() {
    try {
      const sheet = new CSSStyleSheet()
      sheet.replaceSync(CSS)
      document.adoptedStyleSheets = [...document.adoptedStyleSheets, sheet]
    } catch {
      // A WebKit without constructable stylesheets: fall back to an element.
      const style = document.createElement('style')
      style.textContent = CSS
      ;(document.head || document.documentElement).appendChild(style)
    }
  }

  function stamp() {
    const root = document.documentElement
    if (!root) return
    for (const key of Object.keys(ATTRS)) root.toggleAttribute(ATTRS[key], Boolean(niceties[key]))
  }

  // ── Location ──────────────────────────────────────────────────────────────
  // X is a single-page app, so the host only sees full loads. pushState is
  // how every in-app navigation happens; wrapping it is what keeps the
  // sidebar's active row honest.
  let lastUrl = null
  let homeSettled = false

  function report() {
    if (location.href === lastUrl) return
    lastUrl = location.href
    homeSettled = false
    invoke('site_navigated', { url: location.href })
    settleHome()
  }

  for (const method of ['pushState', 'replaceState']) {
    const original = history[method]
    history[method] = function () {
      const result = original.apply(this, arguments)
      queueMicrotask(report)
      return result
    }
  }
  window.addEventListener('popstate', report)

  // ── Following first ───────────────────────────────────────────────────────
  // X resets the home timeline to "For you" on every full load. Once per
  // visit to /home, pick "Following" if it is not already selected. The tab
  // is found by its label, so this is English-only until X exposes a stable id.
  function settleHome() {
    if (!niceties.chronologicalHome || homeSettled || location.pathname !== '/home') return
    const tabs = document.querySelectorAll(
      '[data-testid="primaryColumn"] [role="tablist"] [role="tab"]',
    )
    for (const tab of tabs) {
      if (tab.textContent.trim() !== 'Following') continue
      if (tab.getAttribute('aria-selected') !== 'true') tab.click()
      homeSettled = true
      return
    }
  }

  // ── Profile ───────────────────────────────────────────────────────────────
  // The signed-in handle is the href of X's own profile link. It is what lets
  // the sidebar's Profile row and the profile section detection work.
  let reportedHandle = null

  function findProfile() {
    const link = document.querySelector('a[data-testid="AppTabBar_Profile_Link"]')
    if (!link) return
    const handle = (link.getAttribute('href') || '').replace(/^\//, '')
    if (!handle || handle === reportedHandle) return
    reportedHandle = handle
    invoke('site_profile', { handle })
  }

  // One observer for everything DOM-shaped, coalesced to a frame: X mutates
  // constantly while a timeline streams in.
  let scheduled = false
  const observer = new MutationObserver(() => {
    if (scheduled) return
    scheduled = true
    requestAnimationFrame(() => {
      scheduled = false
      findProfile()
      settleHome()
    })
  })

  function apply(next) {
    niceties = Object.assign({}, niceties, next)
    stamp()
    homeSettled = false
    settleHome()
  }

  function start() {
    stamp()
    observer.observe(document.documentElement, { childList: true, subtree: true })
    findProfile()
    report()
  }

  installStyles()
  if (document.documentElement) start()
  else document.addEventListener('DOMContentLoaded', start, { once: true })

  window.__twister = { apply }
  // The baked-in niceties are what this script was created with; the live
  // ones are whatever the user has set since.
  invoke('site_settings').then((fresh) => {
    if (fresh) apply(fresh)
  })
})()
