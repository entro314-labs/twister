// Twister's capture hook for Meta's sites — Threads and Instagram share it.
// Injected at document start into every Threads and Instagram tab, after
// common.js.
//
// Both sites are the same web stack. A page's first screen arrives inline,
// as Relay's prefetched cache inside `<script type="application/json"
// data-sjs>` tags; everything after that comes from `POST /graphql/query`
// (Threads), `POST /api/graphql` (Instagram) and `/api/v1/…` (Instagram's
// older REST). This script reads all three: the script tags as they land
// and the responses through the shared response hook. The query's name —
// the source — is in the request body (`fb_api_req_friendly_name`), not
// the URL. Shapes read off threads.com and instagram.com on 2026-09-16.
//
// The parse is ignorant of which query answered: any object with a
// `username` and a `pk` is a person; any with a `pk`, a `code` and a
// `user` is a post. Both sites number people and posts with the same pks,
// which is why a row is keyed by network as well.
(() => {
  'use strict'
  if (window.__twisterCapture) return
  const common = window.__twisterCommon
  if (!common) return
  const { str, num, flag, iso, walk } = common

  const API = /\/(?:graphql\/query|api\/graphql|api\/v1\/)/
  const HANDLE = /^[A-Za-z0-9._]{1,30}$/
  const ID = /^\d{1,25}$/
  const CODE = /^[A-Za-z0-9_-]{1,40}$/

  function readUser(node) {
    if (!node || typeof node !== 'object') return null
    const handle = str(node.username, 30)
    const id = str(node.pk !== undefined ? String(node.pk) : node.id, 25)
    if (!HANDLE.test(handle) || !ID.test(id)) return null
    const friendship = node.friendship_status && typeof node.friendship_status === 'object' ? node.friendship_status : null
    return {
      id,
      handle,
      name: str(node.full_name, 200),
      bio: str(node.biography, 2000),
      location: '',
      website: str(node.external_url, 500),
      followers: num(node.follower_count),
      following: num(node.following_count),
      posts: num(node.media_count),
      verified: Boolean(node.is_verified),
      protected: Boolean(node.is_private || node.text_post_app_is_private),
      avatar: str(node.profile_pic_url || node.hd_profile_pic_url_info && node.hd_profile_pic_url_info.url, 500),
      createdAt: '',
      followsMe: friendship ? flag(friendship.followed_by) : null,
      followedByMe: friendship ? flag(friendship.following) : null,
    }
  }

  // The largest candidate of an image, the first (highest) of a video.
  function bestImage(versions) {
    const candidates = versions && Array.isArray(versions.candidates) ? versions.candidates : []
    let best = null
    for (const candidate of candidates) {
      if (!candidate || typeof candidate.url !== 'string') continue
      const width = num(candidate.width)
      if (!best || width > best.width) best = { width, url: candidate.url.slice(0, 500) }
    }
    return best ? best.url : ''
  }

  function readMediaOf(node, out) {
    if (!node || typeof node !== 'object') return
    if (Array.isArray(node.carousel_media)) {
      for (const item of node.carousel_media) readMediaOf(item, out)
      return
    }
    const videos = Array.isArray(node.video_versions) ? node.video_versions : []
    const video = videos.find((v) => v && typeof v.url === 'string')
    if (video) {
      out.push({ kind: 'video', url: video.url.slice(0, 500) })
      return
    }
    const image = bestImage(node.image_versions2) || str(node.display_uri, 500)
    if (image) out.push({ kind: 'photo', url: image })
  }

  function readPost(node, into) {
    if (!node || typeof node !== 'object') return
    const id = str(node.pk !== undefined ? String(node.pk) : '', 25)
    const slug = str(node.code, 40)
    if (!ID.test(id) || !CODE.test(slug)) return
    const author = readUser(node.user)
    if (!author) return
    const caption = node.caption && typeof node.caption === 'object' ? node.caption : {}
    const info = node.text_post_app_info && typeof node.text_post_app_info === 'object' ? node.text_post_app_info : {}
    const share = info.share_info && typeof info.share_info === 'object' ? info.share_info : {}
    let kind = 'post'
    let repostOf = ''
    let quotedId = ''
    if (share.reposted_post && typeof share.reposted_post === 'object') {
      kind = 'repost'
      repostOf = str(String(share.reposted_post.pk || ''), 25)
      readPost(share.reposted_post, into)
    } else if (share.quoted_post && typeof share.quoted_post === 'object') {
      kind = 'quote'
      quotedId = str(String(share.quoted_post.pk || ''), 25)
      readPost(share.quoted_post, into)
    } else if (info.is_reply || info.reply_to_author) {
      kind = 'reply'
    }
    const media = []
    readMediaOf(node, media)
    into.posts.set(id, {
      id,
      slug,
      authorId: author.id,
      authorHandle: author.handle,
      text: str(caption.text, 20000),
      createdAt: iso(node.taken_at),
      kind,
      lang: '',
      likes: num(node.like_count),
      reposts: num(info.repost_count),
      replies: num(info.direct_reply_count, node.comment_count),
      views: num(node.view_count, node.play_count),
      bookmarked: Boolean(node.has_viewer_saved),
      media: media.slice(0, 8),
      repostOf: ID.test(repostOf) ? repostOf : '',
      replyTo: '',
      quotedId: ID.test(quotedId) ? quotedId : '',
    })
    remember(into, author)
  }

  // The same person appears in one answer as a full profile and, on every
  // post, as a stub with a name and a picture. The fuller sighting wins
  // field by field, so a stub seen last does not zero the counts.
  function remember(into, user) {
    const known = into.users.get(user.id)
    if (!known) {
      into.users.set(user.id, user)
      return
    }
    for (const key of Object.keys(user)) {
      const value = user[key]
      if (value === '' || value === 0 || value === null || value === false) continue
      known[key] = value
    }
  }

  function parse(payload) {
    const into = { users: new Map(), posts: new Map() }
    walk(payload, (value) => {
      if (typeof value.username === 'string' && (value.pk !== undefined || typeof value.id === 'string')) {
        const user = readUser(value)
        if (user) remember(into, user)
      }
      if (value.pk !== undefined && typeof value.code === 'string' && value.user) readPost(value, into)
    })
    return into
  }

  const batcher = common.createBatcher()
  const keyOf = (post) => post.slug

  function sourceOf(url, body) {
    const named = /(?:^|&)fb_api_req_friendly_name=([A-Za-z0-9_]+)/.exec(body || '')
    if (named) return named[1]
    const rest = /\/api\/v1\/([A-Za-z0-9_]+)(?:\/([A-Za-z_]+))?/.exec(url)
    if (rest) return rest[2] ? `${rest[1]}.${rest[2]}` : rest[1]
    return 'Graphql'
  }

  // Returns whether the text was whole JSON; a script tag still streaming
  // in is not, and is read again once it is.
  function ingest(text, source) {
    if (!text.includes('"username"')) return true
    // Meta prefixes some JSON answers with `for (;;);`.
    const clean = text.startsWith('for (;;);') ? text.slice(9) : text
    let json
    try {
      json = JSON.parse(clean)
    } catch {
      return false
    }
    batcher.add(source, parse(json), keyOf)
    return true
  }

  common.onResponse((url) => API.test(url), ({ url, text, body }) => {
    ingest(text, sourceOf(url, body))
  })

  // ── The inline cache ──────────────────────────────────────────────────────
  // Relay's prefetched answers, in the page as it arrives and in the script
  // tags the site appends as it navigates. A tag is seen by the observer
  // when it is inserted, before its text has finished arriving, so one that
  // does not parse yet is left for the next pass.
  const read = new WeakSet()
  function readInline() {
    for (const script of document.querySelectorAll('script[type="application/json"][data-sjs]')) {
      if (read.has(script)) continue
      const text = script.textContent || ''
      if (!text.includes('RelayPrefetchedStreamCache')) {
        if (document.readyState !== 'loading') read.add(script)
        continue
      }
      const query = /"queryName":"([A-Za-z0-9_]+)"/.exec(text)
      if (ingest(text, query ? query[1] : 'Inline')) read.add(script)
    }
  }

  // ── The download button ───────────────────────────────────────────────────
  // A post's own link carries its shortcode; the action bar is the row the
  // site's Like glyph sits in. Threads draws posts inside pressable
  // containers, Instagram inside <article>.
  const LINK = /\/(?:post|p|reel)\/([A-Za-z0-9_-]{1,40})(?:[/?#]|$)/
  function postOf(article) {
    for (const link of article.querySelectorAll('a[href*="/post/"], a[href*="/p/"], a[href*="/reel/"]')) {
      const href = link.getAttribute('href') || ''
      const match = LINK.exec(href)
      if (!match) continue
      const author = /^\/@?([A-Za-z0-9._]{1,30})\//.exec(href)
      return { key: match[1], handle: author ? author[1] : '' }
    }
    return null
  }

  function barOf(article) {
    const like = article.querySelector('svg[aria-label="Like"], svg[aria-label="Unlike"]')
    const control = like && like.closest('div[role="button"], button')
    return control && control.parentElement ? control.parentElement.parentElement : null
  }

  const buttons = common.installDownloadButtons({
    article: '[data-pressable-container="true"], article',
    barOf,
    postOf,
    media: batcher.media,
  })

  function start() {
    readInline()
    common.observe(readInline)
    // The page is whole: whatever is still unread is read as it stands.
    document.addEventListener('DOMContentLoaded', readInline, { once: true })
  }
  if (document.documentElement) start()
  else document.addEventListener('DOMContentLoaded', start, { once: true })

  window.__twisterCapture = {
    count: batcher.count,
    flush: batcher.flush,
    setDownloadButton: buttons.setEnabled,
  }
})()
