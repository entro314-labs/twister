// Twister's capture hook for Bluesky. Injected at document start into every
// Bluesky tab, after common.js.
//
// The app talks XRPC — `/xrpc/app.bsky.feed.getAuthorFeed` and the like —
// to the public AppView when signed out and to the account's own PDS when
// signed in, on whatever host that is. This script reads every XRPC answer
// through the shared response hook and hands the people and posts in it to
// the store. Shapes read off public.api.bsky.app on 2026-09-16.
//
// The parse is ignorant of which method answered: any object carrying a
// `did` and a `handle` is a person; any object carrying an at-URI, an
// `author` and a `record.text` is a post. A person's id is their DID, a
// post's its at-URI — the rkey a URL carries is the URI's last segment.
(() => {
  'use strict'
  if (window.__twisterCapture) return
  const common = window.__twisterCommon
  if (!common) return
  const { str, num, iso, walk } = common

  const API = /\/xrpc\/((?:app\.bsky|com\.atproto|chat\.bsky)\.[A-Za-z0-9.]+)/
  const DID = /^did:[a-z]+:[A-Za-z0-9._:%-]{1,200}$/
  const POST_URI = /^at:\/\/did:[a-z]+:[A-Za-z0-9._:%-]+\/app\.bsky\.feed\.post\/[a-z2-7]{1,20}$/

  function readUser(node) {
    if (!node || typeof node !== 'object') return null
    const id = str(node.did, 256)
    const handle = str(node.handle, 253)
    if (!DID.test(id) || !handle.includes('.')) return null
    const viewer = node.viewer && typeof node.viewer === 'object' ? node.viewer : null
    const verification = node.verification && typeof node.verification === 'object' ? node.verification : {}
    return {
      id,
      handle,
      name: str(node.displayName, 200),
      bio: str(node.description, 2000),
      location: '',
      website: '',
      followers: num(node.followersCount),
      following: num(node.followsCount),
      posts: num(node.postsCount),
      verified: verification.verifiedStatus === 'valid' || verification.trustedVerifierStatus === 'valid',
      protected: false,
      avatar: str(node.avatar, 500),
      createdAt: iso(node.createdAt),
      // The viewer block names the follow records when they exist; it is
      // absent when signed out.
      followsMe: viewer ? Boolean(viewer.followedBy) : null,
      followedByMe: viewer ? Boolean(viewer.following) : null,
    }
  }

  function readMedia(embed, out) {
    if (!embed || typeof embed !== 'object') return
    const type = str(embed.$type, 60)
    if (type === 'app.bsky.embed.images#view' && Array.isArray(embed.images)) {
      for (const image of embed.images) {
        const url = image && str(image.fullsize, 500)
        if (url) out.push({ kind: 'photo', url })
      }
    } else if (type === 'app.bsky.embed.video#view') {
      // An HLS playlist: what the site itself plays. Not an mp4.
      const url = str(embed.playlist, 500)
      if (url) out.push({ kind: 'video', url })
    } else if (type === 'app.bsky.embed.recordWithMedia#view') {
      readMedia(embed.media, out)
    }
  }

  function readPost(node, into) {
    if (!node || typeof node !== 'object') return
    const id = str(node.uri, 256)
    const record = node.record
    if (!POST_URI.test(id) || !record || typeof record !== 'object' || typeof record.text !== 'string') return
    const author = readUser(node.author)
    if (!author) return
    const embed = node.embed && typeof node.embed === 'object' ? node.embed : null
    const embedType = embed ? str(embed.$type, 60) : ''
    let kind = 'post'
    let quotedId = ''
    if (record.reply && typeof record.reply === 'object') {
      kind = 'reply'
    } else if (embedType === 'app.bsky.embed.record#view' || embedType === 'app.bsky.embed.recordWithMedia#view') {
      const quoted = embedType === 'app.bsky.embed.record#view' ? embed.record : embed.record && embed.record.record
      const uri = quoted && str(quoted.uri, 256)
      if (uri && POST_URI.test(uri)) {
        kind = 'quote'
        quotedId = uri
      }
    }
    const media = []
    readMedia(embed, media)
    const viewer = node.viewer && typeof node.viewer === 'object' ? node.viewer : {}
    const parent = record.reply && record.reply.parent
    into.posts.set(id, {
      id,
      slug: '',
      authorId: author.id,
      authorHandle: author.handle,
      text: str(record.text, 20000),
      createdAt: iso(record.createdAt),
      kind,
      lang: Array.isArray(record.langs) ? str(record.langs[0], 16) : '',
      likes: num(node.likeCount),
      reposts: num(node.repostCount),
      replies: num(node.replyCount),
      views: 0,
      bookmarked: Boolean(viewer.bookmarked),
      media: media.slice(0, 8),
      repostOf: '',
      replyTo: parent && POST_URI.test(str(parent.uri, 256)) ? parent.uri : '',
      quotedId,
    })
    into.users.set(author.id, author)
  }

  function parse(payload) {
    const into = { users: new Map(), posts: new Map() }
    walk(payload, (value) => {
      if (typeof value.did === 'string' && typeof value.handle === 'string') {
        const user = readUser(value)
        if (user) into.users.set(user.id, user)
      }
      if (typeof value.uri === 'string' && value.record && value.author) readPost(value, into)
    })
    return into
  }

  const rkeyOf = (uri) => uri.slice(uri.lastIndexOf('/') + 1)
  const batcher = common.createBatcher()
  // rkey → at-URI for every post seen, so a post found in the page by its
  // link can be named to Rust by its id.
  const uris = new Map()

  common.onResponse((url) => API.test(url), ({ url, text }) => {
    if (!text.includes('"did"')) return
    let json
    try {
      json = JSON.parse(text)
    } catch {
      return
    }
    const found = parse(json)
    for (const uri of found.posts.keys()) uris.set(rkeyOf(uri), uri)
    const match = API.exec(url)
    batcher.add((match && match[1]) || 'xrpc', found, (post) => rkeyOf(post.id))
  })

  // ── The download button ───────────────────────────────────────────────────
  // Posts are `feedItem-by-<handle>` in a feed and `postThreadItem-by-<handle>`
  // in a thread; the post's own link carries the rkey. The action bar is the
  // row the like button sits in.
  function postOf(article) {
    for (const link of article.querySelectorAll('a[href*="/post/"]')) {
      const match = /^\/profile\/([^/]+)\/post\/([a-z2-7]{1,20})(?:[/?#]|$)/.exec(link.getAttribute('href') || '')
      if (match) return { key: match[2], handle: match[1] }
    }
    return null
  }

  const buttons = common.installDownloadButtons({
    article: '[data-testid^="feedItem-by-"], [data-testid^="postThreadItem-by-"]',
    barOf: (article) => {
      const like = article.querySelector('[data-testid="likeBtn"]')
      return like && like.parentElement ? like.parentElement.parentElement : null
    },
    postOf,
    media: batcher.media,
  })

  window.__twisterCapture = {
    count: batcher.count,
    flush: batcher.flush,
    setDownloadButton: buttons.setEnabled,
    uriOf: (rkey) => uris.get(rkey) || '',
  }
})()
