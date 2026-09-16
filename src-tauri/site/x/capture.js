// Twister's capture hook for X. Injected at document start into every X tab,
// after common.js.
//
// X's page fetches its own API; this script reads the answers as they
// arrive (through the shared response hook) and hands the people and posts
// in them to the store. No request is ever made on the page's behalf, no
// token is read, and nothing is sent anywhere but the app.
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
  const common = window.__twisterCommon
  if (!common) return
  const { str, num, flag, iso, walk } = common

  // X's API lives at /i/api/ on the page's own origin and, increasingly, at
  // api.x.com; the operation name is the last path segment of a GraphQL call.
  const API = /(?:\/i\/api\/|\/\/api\.(?:x|twitter)\.com\/)(?:graphql\/[^/]+\/([A-Za-z0-9_]+)|(?:1\.1|2)\/)/
  const HANDLE = /^[A-Za-z0-9_]{1,15}$/
  const ID = /^\d{1,25}$/

  // ── Reading X's objects ───────────────────────────────────────────────────

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
      slug: '',
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

  function parse(payload) {
    const into = { users: new Map(), posts: new Map() }
    walk(payload, (value) => {
      if (value.__typename === 'User' || (value.legacy && typeof value.legacy.screen_name === 'string') ||
        (value.core && typeof value.core.screen_name === 'string')) {
        const user = readUser(value)
        if (user) into.users.set(user.id, user)
      }
      if (value.__typename === 'Tweet' || value.__typename === 'TweetWithVisibilityResults' ||
        (value.legacy && typeof value.legacy.full_text === 'string')) {
        readPost(value, into)
      }
    })
    return into
  }

  // ── Batching to the app ───────────────────────────────────────────────────

  const batcher = common.createBatcher()

  common.onResponse((url) => API.test(url), ({ url, text }) => {
    // A cheap gate before parsing a megabyte of JSON for nothing.
    if (!text.includes('"screen_name"') && !text.includes('"full_text"')) return
    let json
    try {
      json = JSON.parse(text)
    } catch {
      return
    }
    const match = API.exec(url)
    batcher.add((match && match[1]) || 'Rest', parse(json))
  })

  // ── The download button ───────────────────────────────────────────────────
  // On every post whose action bar is on screen and whose media the hook has
  // seen — or that shows a photo the page drew, which is enough on its own.

  function postIdOf(article) {
    for (const link of article.querySelectorAll('a[href*="/status/"]')) {
      const match = /^\/([A-Za-z0-9_]{1,15})\/status\/(\d{1,25})(?:[/?#]|$)/.exec(link.getAttribute('href') || '')
      if (match && link.querySelector('time')) return { key: match[2], handle: match[1] }
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

  const buttons = common.installDownloadButtons({
    article: 'article[data-testid="tweet"]',
    barOf: (article) => article.querySelector('[role="group"]'),
    postOf: postIdOf,
    pageImages,
    media: batcher.media,
  })

  window.__twisterCapture = {
    count: batcher.count,
    flush: batcher.flush,
    setDownloadButton: buttons.setEnabled,
  }
})()
