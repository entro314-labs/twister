// Twister's capture hook. Injected at document start into every tab.
//
// X's page fetches its own API; this script reads the answers as they
// arrive and hands the people and posts in them to the store. No request is
// ever made on the page's behalf, no token is read, and nothing is sent
// anywhere but the app. The page is patched, not the network: `fetch` and
// `XMLHttpRequest` are wrapped so that a response to X's own GraphQL
// endpoint is cloned and parsed on the side.
//
// The parse is deliberately ignorant of X's schema. Any object carrying a
// `screen_name` (or the newer `core.screen_name`) is a user; any
// `tweet_results.result` (or a `legacy.full_text` with an id) is a post.
// Field names come from what X has used for years, with the newer
// locations checked first.
//
// The same script owns the download button, because the button needs the
// media map this hook builds.
(() => {
  'use strict'
  if (window.__twisterCapture) return

  const internals = window.__TAURI_INTERNALS__
  const invoke = (command, args) =>
    internals ? internals.invoke(command, args).catch(() => undefined) : Promise.resolve(undefined)

  // X's API lives at /i/api/ on the page's own origin and, increasingly, at
  // api.x.com; the operation name is the last path segment of a GraphQL call.
  const API = /(?:\/i\/api\/|\/\/api\.(?:x|twitter)\.com\/)(?:graphql\/[^/]+\/([A-Za-z0-9_]+)|(?:1\.1|2)\/)/
  const MAX_BODY = 12 * 1024 * 1024
  const MAX_DEPTH = 40
  const MAX_NODES = 60000
  const FLUSH_MS = 400
  const FLUSH_AT = 250
  const HANDLE = /^[A-Za-z0-9_]{1,15}$/
  const ID = /^\d{1,25}$/

  // ── Reading X's objects ───────────────────────────────────────────────────

  function str(value, max) {
    return typeof value === 'string' ? value.slice(0, max) : ''
  }

  function num(...values) {
    for (const value of values) {
      if (typeof value === 'number' && Number.isFinite(value)) return Math.trunc(value)
      if (typeof value === 'string' && value.trim() !== '') {
        const parsed = Number(value.replace(/,/g, ''))
        if (Number.isFinite(parsed)) return Math.trunc(parsed)
      }
    }
    return 0
  }

  function flag(...values) {
    for (const value of values) if (typeof value === 'boolean') return value
    return null
  }

  // "Wed Oct 10 20:19:24 +0000 2018" → RFC 3339
  function iso(value) {
    if (typeof value !== 'string' || !value) return ''
    const ms = Date.parse(value)
    return Number.isFinite(ms) ? new Date(ms).toISOString().replace(/\.\d{3}Z$/, 'Z') : ''
  }

  function website(legacy) {
    const urls = legacy && legacy.entities && legacy.entities.url && legacy.entities.url.urls
    if (!Array.isArray(urls) || !urls.length) return ''
    return str(urls[0].expanded_url || urls[0].url, 500)
  }

  function readUser(node) {
    if (!node || typeof node !== 'object') return null
    const legacy = node.legacy && typeof node.legacy === 'object' ? node.legacy : node
    const core = node.core && typeof node.core === 'object' ? node.core : {}
    const handle = str(core.screen_name || legacy.screen_name, 15)
    const id = str(node.rest_id || node.id_str || legacy.id_str, 25)
    if (!HANDLE.test(handle) || !ID.test(id)) return null
    const perspectives = node.relationship_perspectives || {}
    const website_ = website(legacy)
    return {
      id,
      handle,
      name: str(core.name || legacy.name, 200),
      bio: str(legacy.description, 2000),
      location: str((node.location && node.location.location) || legacy.location, 200),
      website: website_,
      followers: num(legacy.followers_count, legacy.normal_followers_count),
      following: num(legacy.friends_count, legacy.following_count),
      posts: num(legacy.statuses_count),
      verified: Boolean(
        node.is_blue_verified || legacy.verified || (node.verification && node.verification.verified),
      ),
      protected: Boolean((node.privacy && node.privacy.protected) || legacy.protected),
      avatar: str((node.avatar && node.avatar.image_url) || legacy.profile_image_url_https, 500),
      createdAt: iso(core.created_at || legacy.created_at),
      followsMe: flag(perspectives.followed_by, legacy.followed_by),
      followedByMe: flag(perspectives.following, legacy.following),
    }
  }

  function readMedia(legacy) {
    const items = (legacy.extended_entities && legacy.extended_entities.media) ||
      (legacy.entities && legacy.entities.media) || []
    const out = []
    for (const item of items) {
      if (!item || typeof item !== 'object') continue
      const kind = str(item.type, 20)
      if (kind === 'photo') {
        const url = str(item.media_url_https, 500)
        if (url) out.push({ kind, url: url + (url.includes('?') ? '&' : '?') + 'name=orig' })
        continue
      }
      if (kind === 'video' || kind === 'animated_gif') {
        const variants = (item.video_info && item.video_info.variants) || []
        let best = null
        for (const variant of variants) {
          if (!variant || typeof variant.url !== 'string') continue
          if (variant.content_type !== 'video/mp4' && !/\.mp4(\?|$)/.test(variant.url)) continue
          const bitrate = num(variant.bitrate)
          if (!best || bitrate > best.bitrate) best = { bitrate, url: variant.url.slice(0, 500) }
        }
        if (best) out.push({ kind, url: best.url })
      }
    }
    return out.slice(0, 8)
  }

  function readPost(node, into) {
    if (!node || typeof node !== 'object') return
    // A visibility wrapper: the post is one level down.
    if (node.tweet && typeof node.tweet === 'object' && node.tweet.legacy) node = node.tweet
    const legacy = node.legacy
    if (!legacy || typeof legacy !== 'object' || typeof legacy.full_text !== 'string') return
    const id = str(node.rest_id || legacy.id_str, 25)
    if (!ID.test(id)) return
    const author = readUser(node.core && node.core.user_results && node.core.user_results.result)
    if (!author) return
    const text = (node.note_tweet && node.note_tweet.note_tweet_results &&
      node.note_tweet.note_tweet_results.result && node.note_tweet.note_tweet_results.result.text) ||
      legacy.full_text
    let kind = 'post'
    let repostOf = ''
    const repost = legacy.retweeted_status_result && legacy.retweeted_status_result.result
    if (repost) {
      kind = 'repost'
      repostOf = str(repost.rest_id || (repost.tweet && repost.tweet.rest_id), 25)
      readPost(repost, into)
    } else if (legacy.in_reply_to_status_id_str) {
      kind = 'reply'
    } else if (legacy.is_quote_status) {
      kind = 'quote'
    }
    const quoted = node.quoted_status_result && node.quoted_status_result.result
    if (quoted) readPost(quoted, into)
    into.posts.set(id, {
      id,
      authorId: author.id,
      authorHandle: author.handle,
      text: str(text, 20000),
      createdAt: iso(legacy.created_at),
      kind,
      lang: str(legacy.lang, 16),
      likes: num(legacy.favorite_count),
      reposts: num(legacy.retweet_count),
      replies: num(legacy.reply_count),
      views: num(node.views && node.views.count),
      bookmarked: Boolean(legacy.bookmarked),
      media: readMedia(legacy),
      repostOf,
      replyTo: str(legacy.in_reply_to_status_id_str, 25),
      quotedId: str(legacy.quoted_status_id_str, 25),
    })
    into.users.set(author.id, author)
  }

  // A bounded walk: X's payloads nest deeply but never past MAX_DEPTH, and a
  // response that would take more than MAX_NODES is a timeline the store
  // gets in pieces anyway.
  function walk(payload, into) {
    let nodes = 0
    const seen = new WeakSet()
    const visit = (value, depth) => {
      if (!value || typeof value !== 'object' || depth > MAX_DEPTH || nodes > MAX_NODES) return
      if (seen.has(value)) return
      seen.add(value)
      nodes += 1
      if (Array.isArray(value)) {
        for (const item of value) visit(item, depth + 1)
        return
      }
      if (value.__typename === 'User' || (value.legacy && typeof value.legacy.screen_name === 'string') ||
        (value.core && typeof value.core.screen_name === 'string')) {
        const user = readUser(value)
        if (user) into.users.set(user.id, user)
      }
      if (value.__typename === 'Tweet' || value.__typename === 'TweetWithVisibilityResults' ||
        (value.legacy && typeof value.legacy.full_text === 'string')) {
        readPost(value, into)
      }
      for (const key in value) visit(value[key], depth + 1)
    }
    visit(payload, 0)
  }

  // ── Batching to the app ───────────────────────────────────────────────────

  const pending = { users: new Map(), posts: new Map(), source: '' }
  const media = new Map() // post id → { handle, media }
  let flushTimer = null
  let captured = 0

  function flush() {
    flushTimer = null
    if (!pending.users.size && !pending.posts.size) return
    const batch = {
      source: pending.source || 'Unknown',
      users: [...pending.users.values()],
      posts: [...pending.posts.values()],
    }
    pending.users.clear()
    pending.posts.clear()
    captured += batch.users.length + batch.posts.length
    invoke('site_capture', { batch })
    document.dispatchEvent(new CustomEvent('twister:captured', { detail: captured }))
  }

  function handle(url, text) {
    const match = API.exec(url)
    if (!match) return
    if (typeof text !== 'string' || text.length > MAX_BODY) return
    // A cheap gate before parsing a megabyte of JSON for nothing.
    if (!text.includes('"screen_name"') && !text.includes('"full_text"')) return
    let json
    try {
      json = JSON.parse(text)
    } catch {
      return
    }
    const into = { users: new Map(), posts: new Map() }
    walk(json, into)
    if (!into.users.size && !into.posts.size) return
    const source = match[1] || 'Rest'
    // One batch per source, so a bookmarks page and a timeline that answer
    // in the same tick do not share a label.
    if (pending.source && pending.source !== source) flush()
    pending.source = source
    for (const [id, user] of into.users) pending.users.set(id, user)
    for (const [id, post] of into.posts) {
      pending.posts.set(id, post)
      if (post.media.length) media.set(id, { handle: post.authorHandle, media: post.media })
    }
    if (pending.users.size + pending.posts.size >= FLUSH_AT) flush()
    else if (!flushTimer) flushTimer = setTimeout(flush, FLUSH_MS)
  }

  // ── The hooks ─────────────────────────────────────────────────────────────
  // Wrapped, and made to look unwrapped: X compares nothing today, but a
  // `fetch.toString()` that reads "[native code]" costs nothing.

  const nativeToString = Function.prototype.toString
  function conceal(wrapper, original) {
    try {
      Object.defineProperty(wrapper, 'name', { value: original.name })
      Object.defineProperty(wrapper, 'toString', {
        value: () => nativeToString.call(original),
        writable: false,
      })
    } catch {
      // A frozen prototype; the wrapper works either way.
    }
    return wrapper
  }

  const originalFetch = window.fetch
  window.fetch = conceal(function fetch(...args) {
    const result = originalFetch.apply(this, args)
    result.then((response) => {
      try {
        const url = (response && response.url) ||
          (typeof args[0] === 'string' ? args[0] : args[0] && args[0].url) || ''
        if (response && response.ok && response.clone && API.test(url)) {
          response.clone().text().then((text) => handle(url, text)).catch(() => {})
        }
      } catch {
        // Not our response to break.
      }
    }).catch(() => {})
    return result
  }, originalFetch)

  const originalOpen = XMLHttpRequest.prototype.open
  const originalSend = XMLHttpRequest.prototype.send
  XMLHttpRequest.prototype.open = conceal(function open(_method, url) {
    this.__twisterUrl = typeof url === 'string' ? url : String(url)
    return originalOpen.apply(this, arguments)
  }, originalOpen)
  XMLHttpRequest.prototype.send = conceal(function send() {
    this.addEventListener('load', () => {
      try {
        const url = this.__twisterUrl || this.responseURL || ''
        if (this.status < 200 || this.status >= 300 || !API.test(url)) return
        if (this.responseType === '' || this.responseType === 'text') handle(url, this.responseText)
        else if (this.responseType === 'json' && this.response) handle(url, JSON.stringify(this.response))
      } catch {
        // Same: never the page's problem.
      }
    })
    return originalSend.apply(this, arguments)
  }, originalSend)

  // ── The download button ───────────────────────────────────────────────────
  // On every post whose action bar is on screen and whose media the hook has
  // seen — or that shows a photo the page drew, which is enough on its own.

  const BUTTON_ATTR = 'data-twister-download'
  const DOWNLOAD_GLYPH =
    'M12 15.6l-4.5-4.5 1.4-1.4 2.1 2.1V3h2v8.8l2.1-2.1 1.4 1.4L12 15.6zM4 20v-5h2v3h12v-3h2v5H4z'
  let downloadButton = true

  function postIdOf(article) {
    for (const link of article.querySelectorAll('a[href*="/status/"]')) {
      const match = /^\/([A-Za-z0-9_]{1,15})\/status\/(\d{1,25})(?:[/?#]|$)/.exec(link.getAttribute('href') || '')
      if (match && link.querySelector('time')) return { handle: match[1], id: match[2] }
    }
    return null
  }

  function pageImages(article) {
    const urls = []
    for (const img of article.querySelectorAll('img[src*="pbs.twimg.com/media/"]')) {
      if (img.closest('[data-testid="card.wrapper"]')) continue
      urls.push(img.currentSrc || img.src)
    }
    return urls.slice(0, 8)
  }

  function decorate() {
    if (!downloadButton) return
    for (const article of document.querySelectorAll('article[data-testid="tweet"]')) {
      const bar = article.querySelector('[role="group"]')
      if (!bar || bar.querySelector(`[${BUTTON_ATTR}]`)) continue
      const post = postIdOf(article)
      if (!post) continue
      if (!media.has(post.id) && !pageImages(article).length) continue
      const button = document.createElement('button')
      button.type = 'button'
      button.setAttribute(BUTTON_ATTR, '')
      button.setAttribute('aria-label', 'Download media')
      button.title = 'Download media'
      const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg')
      svg.setAttribute('viewBox', '0 0 24 24')
      svg.setAttribute('aria-hidden', 'true')
      const path = document.createElementNS('http://www.w3.org/2000/svg', 'path')
      path.setAttribute('d', DOWNLOAD_GLYPH)
      svg.appendChild(path)
      button.appendChild(svg)
      button.addEventListener('click', (event) => {
        event.preventDefault()
        event.stopPropagation()
        const known = media.get(post.id)
        invoke('site_download', {
          request: {
            postId: post.id,
            handle: known ? known.handle : post.handle,
            images: pageImages(article),
          },
        })
      })
      bar.appendChild(button)
    }
  }

  let scheduled = false
  const observer = new MutationObserver(() => {
    if (scheduled) return
    scheduled = true
    requestAnimationFrame(() => {
      scheduled = false
      decorate()
    })
  })

  function start() {
    observer.observe(document.documentElement, { childList: true, subtree: true })
    decorate()
  }
  if (document.documentElement) start()
  else document.addEventListener('DOMContentLoaded', start, { once: true })

  window.__twisterCapture = {
    count: () => captured,
    flush,
    setDownloadButton(on) {
      downloadButton = Boolean(on)
      if (!downloadButton) for (const b of document.querySelectorAll(`[${BUTTON_ATTR}]`)) b.remove()
      else decorate()
    },
  }
})()
