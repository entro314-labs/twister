// Twister's bridge into x.com. Injected at document start into the site
// webview, main frame only, on every page load.
//
// Everything that reaches into X's DOM lives here and in niceties.css: X ships
// changes without notice, and a broken nicety should be a one-file fix. Rust
// substitutes the two __TWISTER_*__ placeholders before injection.
//
// The bridge calls four commands — site_settings, site_navigated,
// site_profile and site_layout — and each validates what it is sent, because
// anything running on this page could call them too. The capture hook and the
// operations are separate scripts (capture.js, ops.js).
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
    classicTwitter: 'data-twister-classic',
    fitTimeline: 'data-twister-fit',
    smoothScroll: 'data-twister-smooth',
    compactPosts: 'data-twister-compact',
    squareAvatars: 'data-twister-square',
    actionsOnHover: 'data-twister-actions-hover',
    hideActionCounts: 'data-twister-hide-action-counts',
    starFavorites: 'data-twister-star',
    compactCompose: 'data-twister-compact-compose',
    hideInlineComposer: 'data-twister-hide-composer',
    hidePageHeaders: 'data-twister-hide-headers',
    hideTimelineModules: 'data-twister-hide-modules',
    timeOnRight: 'data-twister-time-right',
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
    const font = typeof niceties.font === 'string' ? niceties.font.trim() : ''
    root.toggleAttribute('data-twister-font', font !== '')
    if (font) root.style.setProperty('--twister-font', font)
    else root.style.removeProperty('--twister-font')
    const size = niceties.textSize === 'small' || niceties.textSize === 'large' ? niceties.textSize : ''
    if (size) root.setAttribute('data-twister-text-size', size)
    else root.removeAttribute('data-twister-text-size')
    stampRoute()
    stampDim()
  }

  // Messages is the one place the right column must stay: it is the
  // conversation. The route is stamped so the CSS can make the exception;
  // Home is stamped so its header, tabs and all, can be the one that goes.
  const CHAT_ROUTE = /^\/(?:i\/chat|messages)(?:\/|$)/
  function stampRoute() {
    const root = document.documentElement
    if (!root) return
    root.toggleAttribute('data-twister-chat', CHAT_ROUTE.test(location.pathname))
    root.toggleAttribute('data-twister-home', location.pathname === '/home')
  }

  // ── Modules ───────────────────────────────────────────────────────────────
  // What X puts between posts: "Who to follow", "Discover more", news,
  // premium prompts. A timeline is a list of cells; a module is a heading
  // cell and the cells after it that hold no post, or a prompt cell, or a
  // person cell. Under a post, "Discover more" — a heading with a line of
  // description beside it — is where the conversation ends and X's
  // suggestions begin, so everything after it goes too. Only on the pages
  // that are timelines of posts: search and notifications keep theirs.
  const TIMELINE = '[data-testid="primaryColumn"] section > h1 + div[aria-label] > div'
  const MODULE_ROUTE = /^\/(?:home|i\/(?:bookmarks|history|lists\/\d+)|[A-Za-z0-9_]{1,15}(?:\/(?:with_replies|highlights|media|likes|status\/\d+))?)\/?$/
  // Reserved paths a handle's shape would otherwise match.
  const NOT_A_PROFILE = /^\/(?:home|explore|notifications|search|messages|settings|compose|jobs|business|login|signup|about|privacy|tos)(?:\/|$)/
  const MODULE_CELL = '[data-testid="inlinePrompt"], [data-testid="UserCell"], a[href^="/i/premium"]'
  function isModuleRoute(path) {
    return path === '/home' || (MODULE_ROUTE.test(path) && !NOT_A_PROFILE.test(path))
  }
  function markModules() {
    if (!niceties.hideTimelineModules || !isModuleRoute(location.pathname)) return
    // Only under a post does a described heading end the conversation;
    // on Home a heading is one module, and the posts after it are posts.
    const underPost = /\/status\/\d+/.test(location.pathname)
    for (const timeline of document.querySelectorAll(TIMELINE)) {
      let inModule = false
      let afterDiscover = false
      for (const item of timeline.children) {
        let hide = afterDiscover
        if (!hide) {
          const heading = item.querySelector('h2[role="heading"]')
          if (item.querySelector('article')) {
            inModule = false
          } else if (heading) {
            const described = heading.nextElementSibling
            afterDiscover = underPost && Boolean(described && described.tagName === 'DIV' && described.hasAttribute('dir'))
            inModule = true
            hide = true
          } else if (item.querySelector(MODULE_CELL) || inModule) {
            hide = true
          }
        }
        item.toggleAttribute('data-twister-module', hide)
      }
    }
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
    const on = Boolean(niceties.classicTwitter)
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

  // ── The star ──────────────────────────────────────────────────────────────
  // The like button carries a test id, so its glyph is swapped whatever
  // heart X drew: an outline when unlit, filled when lit. The heart's own
  // path is kept on the element and restored when the switch goes off.
  const STAR =
    'M12 2.6l2.95 6.28 6.85.82-5.06 4.7 1.34 6.8L12 17.8l-6.08 3.4 1.34-6.8L2.2 9.7l6.85-.82L12 2.6z'
  const LIKE_PATHS = '[data-testid="like"] svg path, [data-testid="unlike"] svg path'
  function swapStar() {
    const on = Boolean(niceties.starFavorites)
    for (const path of document.querySelectorAll(on ? LIKE_PATHS : '[data-twister-heart]')) {
      const d = path.getAttribute('d') || ''
      if (on) {
        // X redraws the heart on every like and unlike; whatever it drew last
        // is what goes back.
        if (d !== STAR) path.setAttribute('data-twister-heart', d)
        const lit = Boolean(path.closest('[data-testid="unlike"]'))
        if (d !== STAR) path.setAttribute('d', STAR)
        path.setAttribute('fill', lit ? 'currentColor' : 'none')
        path.setAttribute('stroke', 'currentColor')
        path.setAttribute('stroke-width', '1.75')
        path.setAttribute('stroke-linejoin', 'round')
      } else {
        path.setAttribute('d', path.getAttribute('data-twister-heart') || d)
        for (const attribute of ['data-twister-heart', 'fill', 'stroke', 'stroke-width', 'stroke-linejoin']) path.removeAttribute(attribute)
      }
    }
  }

  // ── The count ──────────────────────────────────────────────────────────────
  // X draws its count as a ring with no number on it and no ARIA. The quiet
  // composer hides the ring and writes what is left beside it, counted by
  // X's rule — the rule compose.rs applies to the Write panel: a URL weighs
  // 23, most characters one, CJK and emoji two. Two copies by design: one
  // runs in the page and one in Rust, and they cannot share a function.
  const LIMIT = 280
  const URL_WEIGHT = 23
  const segmenter =
    typeof Intl !== 'undefined' && Intl.Segmenter ? new Intl.Segmenter(undefined, { granularity: 'grapheme' }) : null
  function graphemes(text) {
    if (!segmenter) return [...text]
    const out = []
    for (const { segment } of segmenter.segment(text)) out.push(segment)
    return out
  }
  function weight(grapheme) {
    const cp = grapheme.codePointAt(0)
    if (cp === undefined) return 0
    const light = cp <= 4351 || (cp >= 8192 && cp <= 8205) || (cp >= 8208 && cp <= 8223) || (cp >= 8242 && cp <= 8247)
    return light ? 1 : 2
  }
  function looksLikeUrl(run) {
    const trimmed = run.replace(/[.,)!?;:]+$/, '')
    const scheme = trimmed.match(/^https?:\/\/(.*)$/)
    if (scheme) return scheme[1].includes('.') && !scheme[1].startsWith('.')
    const host = trimmed.split('/')[0]
    if (host.startsWith('@')) return false
    const labels = host.split('.')
    if (labels.length < 2) return false
    const tld = labels[labels.length - 1]
    return /^[a-zA-Z]{2,}$/.test(tld) && labels.slice(0, -1).every((label) => /^[a-zA-Z0-9-]+$/.test(label))
  }
  function weigh(text) {
    let total = 0
    for (const run of text.split(/(\s+)/)) {
      if (run === '') continue
      if (/^\s+$/.test(run)) {
        total += graphemes(run).length
        continue
      }
      const opened = run.length - run.replace(/^[([]+/, '').length
      const closed = run.length - run.replace(/[)\].,!?;:]+$/, '').length
      if (looksLikeUrl(run.slice(opened))) {
        total += opened + URL_WEIGHT + closed
        continue
      }
      for (const grapheme of graphemes(run)) total += weight(grapheme)
    }
    return total
  }

  // The toolbar belongs to whichever part of the thread is being written:
  // the one with focus, else the last.
  const TEXTBOX = '[data-testid^="tweetTextarea_"][role="textbox"]'
  function textboxFor(bar) {
    let node = bar.parentElement
    while (node && !node.querySelector(TEXTBOX)) node = node.parentElement
    if (!node) return null
    const boxes = node.querySelectorAll(TEXTBOX)
    for (const box of boxes) if (box.contains(document.activeElement)) return box
    return boxes[boxes.length - 1] || null
  }
  // The ring is the one SVG in the composer's bottom row that is circles and
  // not a button. X draws that row two ways — the Post button inside the
  // toolbar, or beside it — so the row is whatever holds both.
  const SEND = '[data-testid="tweetButtonInline"], [data-testid="tweetButton"]'
  function rowOf(bar) {
    let node = bar
    while (node && node !== document.body && !node.querySelector(SEND)) node = node.parentElement
    return node && node !== document.body ? node : bar
  }
  function ringOf(row) {
    for (const svg of row.querySelectorAll('svg')) {
      if (svg.querySelector('circle') && !svg.closest('button, [role="button"], a')) return svg
    }
    return null
  }
  function paintCount() {
    const on = Boolean(niceties.compactCompose)
    for (const bar of document.querySelectorAll('[data-testid="toolBar"]')) {
      const row = rowOf(bar)
      let label = row.querySelector('[data-twister-count]')
      const ring = ringOf(row)
      if (ring) ring.toggleAttribute('data-twister-ring', on)
      const box = on ? textboxFor(bar) : null
      const send = row.querySelector(SEND)
      const anchor = ring ? ring.parentElement : send ? send.parentElement : null
      if (!on || !box || !anchor || !anchor.parentElement) {
        if (label) label.remove()
        continue
      }
      if (!label) {
        label = document.createElement('span')
        label.setAttribute('data-twister-count', '')
        anchor.parentElement.insertBefore(label, anchor)
      }
      const left = LIMIT - weigh(box.innerText.replace(/\n$/, ''))
      const text = String(left)
      if (label.textContent !== text) label.textContent = text
      label.toggleAttribute('data-over', left < 0)
      label.toggleAttribute('data-near', left >= 0 && left <= 20)
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

  // ── Classic wording ───────────────────────────────────────────────────────
  // Posts are tweets again, in X's own controls only: buttons, tabs, menu
  // items, the social-context line and the page title. Never in anything a
  // person wrote. English only, because that is the one language whose
  // words can be told apart from names without a table; other languages
  // keep X's wording.
  const WORDS = [
    [/\bReposted\b/g, 'Retweeted'],
    [/\breposted\b/g, 'retweeted'],
    [/\bReposts\b/g, 'Retweets'],
    [/\breposts\b/g, 'retweets'],
    [/\bRepost\b/g, 'Retweet'],
    [/\brepost\b/g, 'retweet'],
    [/\bPosted\b/g, 'Tweeted'],
    [/\bPosts\b/g, 'Tweets'],
    [/\bposts\b/g, 'tweets'],
    [/\bPost\b/g, 'Tweet'],
    [/\bpost\b/g, 'tweet'],
  ]
  const STAR_WORDS = [
    [/\bUnlike\b/g, 'Unfavorite'],
    [/\bLiked\b/g, 'Favorited'],
    [/\bLikes\b/g, 'Favorites'],
    [/\bLike\b/g, 'Favorite'],
  ]
  function activeWords() {
    const words = []
    if (niceties.classicTwitter) words.push(...WORDS)
    if (niceties.starFavorites) words.push(...STAR_WORDS)
    return words
  }
  const WORDING_HOSTS =
    'button, [role="button"], [role="tab"], [role="menuitem"], [data-testid="socialContext"], [data-testid="SideNav_NewTweet_Button"], h2[role="heading"], [data-testid="primaryColumn"] nav a, [data-testid="tweetTextarea_0"][data-placeholder], [aria-label]'
  const PERSON_WROTE =
    '[data-testid="tweetText"], [data-testid="User-Name"], [data-testid="UserName"], [data-testid="UserDescription"], [data-testid="UserCell"] [dir], [contenteditable="true"], [data-testid="tweetTextarea_0"]'
  const english = () => /^en\b/.test(document.documentElement.lang || 'en')

  function reword(text, words) {
    let out = text
    for (const [pattern, replacement] of words) out = out.replace(pattern, replacement)
    return out
  }

  function restoreWording() {
    const words = activeWords()
    if (words.length === 0 || !english()) return
    const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT, {
      acceptNode(node) {
        const parent = node.parentElement
        if (!parent || !/post|like/i.test(node.nodeValue)) return NodeFilter.FILTER_REJECT
        if (parent.closest(PERSON_WROTE)) return NodeFilter.FILTER_REJECT
        return parent.closest(WORDING_HOSTS) ? NodeFilter.FILTER_ACCEPT : NodeFilter.FILTER_REJECT
      },
    })
    const nodes = []
    for (let node = walker.nextNode(); node; node = walker.nextNode()) nodes.push(node)
    for (const node of nodes) {
      const next = reword(node.nodeValue, words)
      if (next !== node.nodeValue) node.nodeValue = next
    }
    for (const element of document.querySelectorAll('[aria-label*="ost"], [aria-label*="ike"], [data-placeholder*="ost"]')) {
      if (element.closest(PERSON_WROTE)) continue
      for (const attribute of ['aria-label', 'data-placeholder']) {
        const value = element.getAttribute(attribute)
        if (value && /post|like/i.test(value)) element.setAttribute(attribute, reword(value, words))
      }
    }
    if (niceties.classicTwitter && document.title && !document.title.includes('Twitter')) {
      const title = document.title
      const next = title === 'X' ? 'Twitter' : reword(title, WORDS).replace(/ \/ X$/, ' / Twitter')
      if (next !== title) document.title = next
    }
  }

  // ── Layout ────────────────────────────────────────────────────────────────
  // How wide X's page wants to be: its main column at X's own 600px, plus
  // the right column when it is showing, plus the gutters. Rust keeps the
  // window at least that wide, so showing the right column again never
  // leaves it cut off.
  const COLUMN_MIN = 600
  const RIGHT_COLUMN = 350
  const GUTTER = 32
  let reportedWidth = 0
  // X caps the column and two wrappers above it. Marking the ancestors up to
  // <main> lets the stylesheet lift every cap without naming a class.
  function markFit() {
    const primary = document.querySelector('[data-testid="primaryColumn"]')
    if (!primary) return
    let node = primary.parentElement
    while (node && node.tagName !== 'MAIN') {
      node.setAttribute('data-twister-fit-col', '')
      node = node.parentElement
    }
  }
  function measureLayout() {
    const primary = document.querySelector('[data-testid="primaryColumn"]')
    if (!primary) return
    if (niceties.fitTimeline) markFit()
    // The right column is counted when the settings want it, even while X
    // has collapsed it for lack of room: the window growing is what brings
    // it back. In Messages the column is the conversation and always counts.
    const right = document.querySelector('[data-testid="sidebarColumn"]')
    const chat = document.documentElement.hasAttribute('data-twister-chat')
    const wantRight = chat || !niceties.hideRightColumn
    const rightWidth = right && right.offsetWidth > 0 ? right.offsetWidth : RIGHT_COLUMN
    const nav = document.querySelector('header[role="banner"]')
    const navVisible = nav && getComputedStyle(nav).display !== 'none' && nav.offsetWidth > 0
    const width =
      COLUMN_MIN +
      (wantRight ? rightWidth + 24 : 0) +
      (navVisible ? nav.offsetWidth : 0) +
      GUTTER
    if (Math.abs(width - reportedWidth) < 1) return
    reportedWidth = width
    // One line describing the column's ancestors, for the debug log: which
    // wrapper caps the width is the first thing to know when X moves it.
    const chain = []
    let node = primary
    while (node && node !== document.body && chain.length < 8) {
      const style = getComputedStyle(node)
      chain.push(`${node.tagName.toLowerCase()}[${node.getAttribute('data-testid') || ''}] ${node.offsetWidth}w max:${style.maxWidth} w:${style.width} ml:${style.marginLeft} mr:${style.marginRight}`)
      node = node.parentElement
    }
    invoke('site_layout', { minWidth: width, detail: chain.join(' < ') })
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
      markModules()
      swapBird()
      swapStar()
      paintCount()
      stampDim()
      restoreWording()
      measureLayout()
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
    if (!niceties.hideTimelineModules) for (const cell of document.querySelectorAll('[data-twister-module]')) cell.removeAttribute('data-twister-module')
    markModules()
    swapBird()
    swapStar()
    paintCount()
    restoreWording()
    reportedWidth = 0
    measureLayout()
    if (window.__twisterCapture) window.__twisterCapture.setDownloadButton(niceties.downloadButton !== false)
  }

  // ── Going places ──────────────────────────────────────────────────────────
  // A full load of an X route is slow and, for the compose modal, never
  // finishes: /compose/post loaded cold sits on the splash screen. X's own
  // navigation links are the in-app transition its router understands, so a
  // destination is reached by clicking the link X drew for it — hidden or
  // not, a click on it still dispatches. For a path X has no link for, a
  // pushState followed by a popstate is what X's own router listens to for
  // back and forward, and it takes the new location the same way. Returns
  // false only when neither is possible; Rust then falls back to a full load.
  function go(path) {
    const link = document.querySelector(`header[role="banner"] a[href="${path}"]`)
    if (link) {
      link.click()
      return true
    }
    if (!/^\/[^\s]*$/.test(path) || !document.querySelector('#react-root')) return false
    history.pushState(null, '', path)
    window.dispatchEvent(new PopStateEvent('popstate', { state: null }))
    return true
  }

  function start() {
    stamp()
    // A like flips the button's test id in place; the star follows it.
    observer.observe(document.documentElement, {
      childList: true,
      subtree: true,
      attributes: true,
      attributeFilter: ['data-testid'],
    })
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
