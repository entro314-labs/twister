// Twister's bridge into instagram.com. Injected at document start into every
// Instagram tab, main frame only, on every page load, after common.js.
//
// Instagram is the same Meta web stack as Threads: generated class names,
// no test ids, a navigation of icons named by their SVG's aria-label.
// Twister changes nothing about the page's layout here — only the font and
// the smoothness of scrolling — and watches the rest. Rust substitutes the
// three __TWISTER_*__ placeholders.
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
  const USER_CSS = __TWISTER_USER_CSS__
  let niceties = __TWISTER_NICETIES__

  function installStyles() {
    common.installSheet('instagram/niceties.css', CSS)
    for (const [name, css] of USER_CSS) common.installSheet(name, css)
  }

  function stamp() {
    const root = document.documentElement
    if (!root) return
    root.toggleAttribute('data-twister-smooth', Boolean(niceties.smoothScroll))
    const font = typeof niceties.font === 'string' ? niceties.font.trim() : ''
    root.toggleAttribute('data-twister-font', font !== '')
    if (font) root.style.setProperty('--twister-font', font)
    else root.style.removeProperty('--twister-font')
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
  // The signed-in handle is the href of the navigation's Profile link — the
  // one whose glyph is labelled Profile. Reserved paths never carry it.
  const NOT_A_PROFILE = /^(?:explore|direct|reels|accounts|p|reel|stories|legal)$/
  let reportedHandle = null
  function findProfile() {
    for (const glyph of document.querySelectorAll('svg[aria-label="Profile"]')) {
      const link = glyph.closest('a[href^="/"]')
      if (!link) continue
      const match = /^\/([A-Za-z0-9._]{1,30})\/?(?:[?#]|$)/.exec(link.getAttribute('href') || '')
      if (!match || NOT_A_PROFILE.test(match[1]) || match[1] === reportedHandle) return
      reportedHandle = match[1]
      invoke('site_profile', { handle: match[1] })
      return
    }
  }

  // ── Going places ──────────────────────────────────────────────────────────
  function go(path) {
    const link = document.querySelector(`a[href="${path}"]`)
    if (link) {
      link.click()
      return true
    }
    if (!/^\/[^\s]*$/.test(path)) return false
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
