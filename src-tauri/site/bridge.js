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
  // [name, css] pairs from the user's styles folder, applied after ours.
  const USER_CSS = __TWISTER_USER_CSS__
  let niceties = __TWISTER_NICETIES__

  const ATTRS = {
    hidePromoted: 'data-twister-hide-promoted',
    hideRightColumn: 'data-twister-hide-right-column',
    hideExtrasNav: 'data-twister-hide-extras-nav',
    hideViewCounts: 'data-twister-hide-view-counts',
    hideSiteNav: 'data-twister-hide-site-nav',
    hideDrawers: 'data-twister-hide-drawers',
    classicBird: 'data-twister-classic-bird',
  }

  function installSheet(name, css) {
    try {
      const sheet = new CSSStyleSheet()
      sheet.replaceSync(css)
      document.adoptedStyleSheets = [...document.adoptedStyleSheets, sheet]
    } catch (err) {
      console.error('[twister] style ' + name + ' failed:', err)
    }
  }

  function installStyles() {
    installSheet('niceties.css', CSS)
    for (const [name, css] of USER_CSS) installSheet(name, css)
  }

  function stamp() {
    const root = document.documentElement
    if (!root) return
    for (const key of Object.keys(ATTRS)) root.toggleAttribute(ATTRS[key], Boolean(niceties[key]))
    stampRoute()
    stampDim()
  }

  // Messages is the one place the right column must stay: it is the
  // conversation. The route is stamped so the CSS can make the exception.
  const CHAT_ROUTE = /^\/(?:i\/chat|messages)(?:\/|$)/
  function stampRoute() {
    const root = document.documentElement
    if (root) root.toggleAttribute('data-twister-chat', CHAT_ROUTE.test(location.pathname))
  }

  // ── Marking ───────────────────────────────────────────────────────────────
  // What CSS alone cannot select. Each pass stamps an attribute the sheet
  // keys on, and is cheap enough to run on every DOM settle.

  // A second net for promoted posts: X's own "Ad" marker, wherever it appears
  // outside a video player's controls, hides the timeline cell around it.
  const PROMOTED_MARKER = '[data-testid="promotedIndicator"], [data-testid="promotedLabel"]'
  const PLAYER = '[data-testid="videoPlayer"], [data-testid="videoComponent"], [data-testid="mediaViewer"]'
  function markPromoted() {
    if (!niceties.hidePromoted) return
    for (const marker of document.querySelectorAll(PROMOTED_MARKER)) {
      if (marker.closest(PLAYER)) continue
      const cell = marker.closest('[data-testid="cellInnerDiv"]')
      if (cell) cell.setAttribute('data-twister-promoted', '')
    }
  }

  // Grok's entry points, found by what they are rather than what they say:
  // the /i/grok route (inside X's chrome, never in a post's text — /grok is
  // a user), and the Grok glyph itself in any button. Locale-independent.
  const GROK_HREF = /^(?:https:\/\/(?:www\.)?x\.com)?\/i\/grok(?:[/?#]|$)/
  const GROK_GLYPH = 'svg path[d^="M12.745 20.54l10.97-8.19"]'
  const CHROME = 'nav, [role="navigation"], [role="menu"], [data-testid="tweet"], [data-testid="HoverCard"], [data-testid="sidebarColumn"]'
  const USER_CONTENT = '[data-testid="tweetText"], [data-testid="User-Name"], [data-testid="UserName"], [data-testid="UserDescription"], [data-testid="UserCell"]'
  function markGrok() {
    if (!niceties.hideExtrasNav) return
    for (const link of document.querySelectorAll('a[href*="grok"]')) {
      if (link.closest(USER_CONTENT) || !link.closest(CHROME)) continue
      link.toggleAttribute('data-twister-grok', GROK_HREF.test(link.getAttribute('href') || ''))
    }
    for (const path of document.querySelectorAll(GROK_GLYPH)) {
      const control = path.closest('button, [role="button"], a[href]')
      if (control && !control.closest(USER_CONTENT)) control.setAttribute('data-twister-grok', '')
    }
    // The premium card in the right column: whichever aside holds the
    // sign-up link. Found by the link, so the heading's language is moot.
    for (const link of document.querySelectorAll('[data-testid="sidebarColumn"] a[href^="/i/premium_sign_up"]')) {
      const card = link.closest('aside, [data-testid="sidebarColumn"] > div > div > div > div')
      if (card && card.getAttribute('data-testid') !== 'sidebarColumn') card.setAttribute('data-twister-grok', '')
    }
  }

  // ── The bird ──────────────────────────────────────────────────────────────
  // The mark is an inline SVG path in X's header and on its loading splash.
  // Swapped path for path, and only the exact current X mark, so a changed
  // artwork leaves the X in place rather than drawing something wrong.
  const X_MARK =
    'M21.742 21.75l-7.563-11.179 7.056-8.321h-2.456l-5.691 6.714-4.54-6.714H2.359l7.29 10.776L2.25 21.75h2.456l6.035-7.118 4.818 7.118h6.191-.008zM7.739 3.818L18.81 20.182h-2.447L5.29 3.818h2.447z'
  const BIRD =
    'M23.643 4.937a9.65 9.65 0 0 1-2.825.775 4.958 4.958 0 0 0 2.163-2.723 9.99 9.99 0 0 1-3.127 1.195 4.916 4.916 0 0 0-8.384 4.482A13.944 13.944 0 0 1 1.64 3.162a4.916 4.916 0 0 0 1.523 6.558 4.903 4.903 0 0 1-2.229-.616v.061a4.917 4.917 0 0 0 3.946 4.818 4.935 4.935 0 0 1-2.224.084 4.923 4.923 0 0 0 4.6 3.419A9.869 9.869 0 0 1 0 19.523a13.94 13.94 0 0 0 7.548 2.212c9.057 0 14.01-7.503 14.01-14.01 0-.213-.005-.425-.014-.636a10.012 10.012 0 0 0 2.46-2.548l-.047-.02z'
  const MARK_HOSTS = 'header[role="banner"] a[aria-label="X"] svg path, #placeholder svg path, [data-testid="SideNav_NewTweet_Button"] ~ * svg path'
  function swapBird() {
    const on = Boolean(niceties.classicBird)
    for (const path of document.querySelectorAll(MARK_HOSTS)) {
      const d = path.getAttribute('d')
      if (on && d === X_MARK) {
        path.setAttribute('d', BIRD)
        path.setAttribute('data-twister-bird', '')
        path.style.fill = '#1d9bf0'
      } else if (!on && path.hasAttribute('data-twister-bird')) {
        path.setAttribute('d', X_MARK)
        path.removeAttribute('data-twister-bird')
        path.style.fill = ''
      }
    }
  }

  // ── Dim ───────────────────────────────────────────────────────────────────
  // X retired Dim in favour of Lights out. This brings it back without
  // knowing a single class name: X's stylesheet is read for every rule that
  // paints a Lights-out colour, and each gets a twin under
  // html[data-twister-dim] painting the Dim colour instead. Only over Lights
  // out — over X's light theme the mapping would make no sense, so the
  // attribute is stamped from what X painted on <body>.
  const DIM = [
    ['background-color', 'rgb(0, 0, 0)', '#15202b'],
    ['background-color', 'rgb(22, 24, 28)', '#1e2732'],
    ['background-color', 'rgb(32, 35, 39)', '#253341'],
    ['background-color', 'rgb(47, 51, 54)', '#38444d'],
    ['background-color', 'rgba(0, 0, 0, 0.65)', 'rgba(91, 112, 131, 0.4)'],
    ['background-color', 'rgba(0, 0, 0, 0.75)', 'rgba(91, 112, 131, 0.5)'],
    ['border-color', 'rgb(47, 51, 54)', '#38444d'],
    ['border-color', 'rgb(32, 35, 39)', '#38444d'],
    ['border-top-color', 'rgb(47, 51, 54)', '#38444d'],
    ['border-bottom-color', 'rgb(47, 51, 54)', '#38444d'],
    ['border-left-color', 'rgb(47, 51, 54)', '#38444d'],
    ['border-right-color', 'rgb(47, 51, 54)', '#38444d'],
    ['color', 'rgb(231, 233, 234)', '#f7f9f9'],
    ['color', 'rgb(113, 118, 123)', '#8b98a5'],
    ['fill', 'rgb(231, 233, 234)', '#f7f9f9'],
    ['fill', 'rgb(113, 118, 123)', '#8b98a5'],
  ]
  let dimSheet = null
  const dimCounted = new WeakMap()

  function lightsOut() {
    const body = document.body
    return Boolean(body) && body.style.backgroundColor === 'rgb(0, 0, 0)'
  }

  function stampDim() {
    const root = document.documentElement
    if (!root) return
    const on = Boolean(niceties.dim) && lightsOut()
    root.toggleAttribute('data-twister-dim', on)
    if (on) buildDim()
  }

  function buildDim() {
    if (!dimSheet) {
      dimSheet = new CSSStyleSheet()
      document.adoptedStyleSheets = [...document.adoptedStyleSheets, dimSheet]
    }
    for (const sheet of document.styleSheets) {
      let rules
      try {
        rules = sheet.cssRules
      } catch {
        continue
      }
      // X appends rules to one sheet as it goes; only the new tail is read.
      const from = dimCounted.get(sheet) || 0
      if (rules.length <= from) continue
      dimCounted.set(sheet, rules.length)
      for (let i = from; i < rules.length; i += 1) {
        const rule = rules[i]
        if (!rule.style || !rule.selectorText || rule.selectorText.startsWith('html[data-twister-dim]')) continue
        for (const [prop, from_, to] of DIM) {
          if (rule.style.getPropertyValue(prop) !== from_) continue
          try {
            dimSheet.insertRule(`html[data-twister-dim] ${rule.selectorText} { ${prop}: ${to} !important; }`, dimSheet.cssRules.length)
          } catch {
            // A selector the engine will not take back; nothing to paint.
          }
        }
      }
    }
  }

  // ── Location ──────────────────────────────────────────────────────────────
  // X is a single-page app, so the host only sees full loads. pushState is
  // how every in-app navigation happens; wrapping it is what keeps the
  // sidebar's active row honest.
  let lastUrl = null
  let homeSettled = false
  let homeNudges = 0

  function report() {
    if (location.href === lastUrl) return
    lastUrl = location.href
    homeSettled = false
    homeNudges = 0
    invoke('site_navigated', { url: location.href })
    stampRoute()
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
  // visit to /home, pick "Following" if it is not already selected. X redraws
  // the tab strip while it hydrates and a click on the first strip is lost,
  // so the visit is settled only once X itself reports the tab selected —
  // with a cap, so a person choosing "For you" is not fought.
  const HOME_NUDGES = 6
  function settleHome() {
    if (!niceties.chronologicalHome || homeSettled || location.pathname !== '/home') return
    const tabs = document.querySelectorAll(
      '[data-testid="primaryColumn"] [role="tablist"] [role="tab"]',
    )
    // By label where the UI is English, by position otherwise: X draws
    // "For you" first and "Following" second, whatever the language.
    let following = null
    for (const tab of tabs) if (tab.textContent.trim() === 'Following') following = tab
    if (!following && tabs.length >= 2) following = tabs[1]
    if (!following) return
    if (following.getAttribute('aria-selected') === 'true') {
      homeSettled = true
      return
    }
    if (homeNudges >= HOME_NUDGES) {
      homeSettled = true
      return
    }
    homeNudges += 1
    following.click()
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
      markPromoted()
      markGrok()
      swapBird()
      stampDim()
    })
  })

  function apply(next) {
    niceties = Object.assign({}, niceties, next)
    stamp()
    homeSettled = false
    homeNudges = 0
    settleHome()
    markPromoted()
    markGrok()
    swapBird()
  }

  // ── Going places ──────────────────────────────────────────────────────────
  // A full load of an X route is slow and, for the compose modal, never
  // finishes: /compose/post loaded cold sits on the splash screen. X's own
  // navigation links are the in-app transition its router understands, so a
  // destination is reached by clicking the link X drew for it — hidden or
  // not, a click on it still dispatches. Returns false when X has no link
  // for the path, and Rust falls back to a full load.
  function go(path) {
    const link = document.querySelector(`header[role="banner"] a[href="${path}"]`)
    if (!link) return false
    link.click()
    return true
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

  window.__twister = { apply, go }
  // The baked-in niceties are what this script was created with; the live
  // ones are whatever the user has set since.
  invoke('site_settings').then((fresh) => {
    if (fresh) apply(fresh)
  })
})()
