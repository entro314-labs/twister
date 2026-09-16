// Twister's bridge into bsky.app. Injected at document start into every
// Bluesky tab, main frame only, on every page load, after common.js.
//
// Bluesky's web app is React Native for web: a single-page app that
// navigates by pushState and stamps stable `data-testid`s on what it
// draws. What Twister changes here is small — its own navigation can go,
// the font and text size — and every selector is in this file and
// niceties.css. Rust substitutes the three __TWISTER_*__ placeholders.
//
// The bridge calls three commands — site_settings, site_navigated and
// site_profile — and each validates what it is sent against this tab's
// network, because anything running on this page could call them too.
(() => {
  'use strict'
  if (window.__twister) return
  const common = window.__twisterCommon
  if (!common) return
  const { invoke } = common

  const CSS = __TWISTER_CSS__
  // [name, css] pairs from the user's styles folder, applied after ours.
  const USER_CSS = __TWISTER_USER_CSS__
  let niceties = __TWISTER_NICETIES__

  // The switches that mean something on this site. The rest of the prefs
  // object is X's and is ignored here.
  const ATTRS = {
    hideSiteNav: 'data-twister-hide-site-nav',
    smoothScroll: 'data-twister-smooth',
    squareAvatars: 'data-twister-square',
  }

  function installStyles() {
    common.installSheet('bluesky/niceties.css', CSS)
    for (const [name, css] of USER_CSS) common.installSheet(name, css)
  }

  function stamp() {
    const root = document.documentElement
    if (!root) return
    for (const key of Object.keys(ATTRS)) root.toggleAttribute(ATTRS[key], Boolean(niceties[key]))
    const font = typeof niceties.font === 'string' ? niceties.font.trim() : ''
    root.toggleAttribute('data-twister-font', font !== '')
    if (font) root.style.setProperty('--twister-font', font)
    else root.style.removeProperty('--twister-font')
    const size = niceties.textSize === 'small' || niceties.textSize === 'large' ? niceties.textSize : ''
    if (size) root.setAttribute('data-twister-text-size', size)
    else root.removeAttribute('data-twister-text-size')
  }

  // ── Location ──────────────────────────────────────────────────────────────
  let lastUrl = null
  function report() {
    if (location.href === lastUrl) return
    lastUrl = location.href
    invoke('site_navigated', { url: location.href })
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

  // ── Profile ───────────────────────────────────────────────────────────────
  // The signed-in handle: the app keeps its session in localStorage under
  // BSKY_STORAGE, and draws a Profile link in its navigation. Either will do;
  // the storage is there before the navigation is.
  let reportedHandle = null

  function storedHandle() {
    try {
      const raw = localStorage.getItem('BSKY_STORAGE')
      if (!raw) return ''
      const parsed = JSON.parse(raw)
      const account = parsed && parsed.session && parsed.session.currentAccount
      return account && typeof account.handle === 'string' ? account.handle : ''
    } catch {
      return ''
    }
  }

  function linkedHandle() {
    const link = document.querySelector('nav[role="navigation"] a[aria-label="Profile"][href^="/profile/"], nav[role="navigation"] a[href^="/profile/"]')
    if (!link) return ''
    const match = /^\/profile\/([^/?#]+)/.exec(link.getAttribute('href') || '')
    return match && !match[1].startsWith('did:') ? match[1] : ''
  }

  function findProfile() {
    const handle = storedHandle() || linkedHandle()
    if (!handle || handle === reportedHandle) return
    reportedHandle = handle
    invoke('site_profile', { handle })
  }

  // ── Going places ──────────────────────────────────────────────────────────
  // The app's own navigation links are the in-app transition its router
  // understands; for a path it has no link for, a pushState followed by a
  // popstate is what React Navigation listens to. Returns false only when
  // neither is possible; Rust then falls back to a full load.
  function go(path) {
    const bare = path.split('?')[0]
    const link = document.querySelector(`nav[role="navigation"] a[href="${bare}"]`)
    if (link && bare === path) {
      link.click()
      return true
    }
    if (!/^\/[^\s]*$/.test(path) || !document.querySelector('#root')) return false
    history.pushState(null, '', path)
    window.dispatchEvent(new PopStateEvent('popstate', { state: null }))
    return true
  }

  function apply(next) {
    niceties = Object.assign({}, niceties, next)
    stamp()
    if (window.__twisterCapture) window.__twisterCapture.setDownloadButton(niceties.downloadButton !== false)
  }

  function start() {
    stamp()
    common.observe(findProfile)
    findProfile()
    report()
  }

  installStyles()
  if (document.documentElement) start()
  else document.addEventListener('DOMContentLoaded', start, { once: true })

  window.__twister = { apply, go }
  invoke('site_settings').then((fresh) => {
    if (fresh) apply(fresh)
  })
})()
