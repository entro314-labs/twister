// Twister's shared plumbing. Injected first, at document start, into every
// tab of every network, before that network's bridge, capture and operations
// scripts. Nothing in here names a site's DOM or its API: it is the response
// hook, the batcher that feeds the store, the download button's mechanics
// and the operations runner, each taking what is site-specific from the
// network's own script.
//
// Every command reached from here — site_capture, site_download,
// site_op_progress — validates its input in Rust against the calling tab's
// network, because anything running on the page could call it too.
(() => {
  'use strict'
  if (window.__twisterCommon) return

  const internals = window.__TAURI_INTERNALS__
  const invoke = (command, args) =>
    internals ? internals.invoke(command, args).catch(() => undefined) : Promise.resolve(undefined)

  // ── Styles ────────────────────────────────────────────────────────────────
  // A constructed stylesheet, which a site's content-security policy cannot
  // block the way it could an inline <style>.
  function installSheet(name, css) {
    try {
      const sheet = new CSSStyleSheet()
      sheet.replaceSync(css)
      document.adoptedStyleSheets = [...document.adoptedStyleSheets, sheet]
    } catch (err) {
      console.error('[twister] style ' + name + ' failed:', err)
    }
  }

  // ── The response hook ─────────────────────────────────────────────────────
  // `fetch` and `XMLHttpRequest` are wrapped so that a successful response
  // to a URL a listener asked for is cloned and read on the side. No request
  // is ever made on the page's behalf, no token is read. Wrapped, and made
  // to look unwrapped: a `fetch.toString()` that reads "[native code]" costs
  // nothing.
  const MAX_BODY = 12 * 1024 * 1024
  const listeners = [] // { test(url) → bool, handle({ url, text, method, body }) }

  function onResponse(test, handle) {
    listeners.push({ test, handle })
  }

  function deliver(url, text, method, body) {
    if (typeof text !== 'string' || text.length > MAX_BODY) return
    for (const listener of listeners) {
      try {
        if (listener.test(url)) listener.handle({ url, text, method, body })
      } catch {
        // A listener's problem is never the page's.
      }
    }
  }

  function wanted(url) {
    for (const listener of listeners) if (listener.test(url)) return true
    return false
  }

  // A request body as text, when it is text: what Meta's GraphQL puts the
  // query name in.
  function bodyText(body) {
    if (typeof body === 'string') return body
    if (body instanceof URLSearchParams) return body.toString()
    return ''
  }

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
        if (response && response.ok && response.clone && wanted(url)) {
          const init = args[1] || {}
          const method = init.method || (args[0] && args[0].method) || 'GET'
          const body = bodyText(init.body)
          response.clone().text().then((text) => deliver(url, text, method, body)).catch(() => {})
        }
      } catch {
        // Not our response to break.
      }
    }).catch(() => {})
    return result
  }, originalFetch)

  const originalOpen = XMLHttpRequest.prototype.open
  const originalSend = XMLHttpRequest.prototype.send
  XMLHttpRequest.prototype.open = conceal(function open(method, url) {
    this.__twisterUrl = typeof url === 'string' ? url : String(url)
    this.__twisterMethod = typeof method === 'string' ? method : 'GET'
    return originalOpen.apply(this, arguments)
  }, originalOpen)
  XMLHttpRequest.prototype.send = conceal(function send(body) {
    const sent = bodyText(body)
    this.addEventListener('load', () => {
      try {
        const url = this.__twisterUrl || this.responseURL || ''
        if (this.status < 200 || this.status >= 300 || !wanted(url)) return
        const method = this.__twisterMethod || 'GET'
        if (this.responseType === '' || this.responseType === 'text') deliver(url, this.responseText, method, sent)
        else if (this.responseType === 'json' && this.response) deliver(url, JSON.stringify(this.response), method, sent)
      } catch {
        // Same: never the page's problem.
      }
    })
    return originalSend.apply(this, arguments)
  }, originalSend)

  // ── The batcher ───────────────────────────────────────────────────────────
  // People and posts a reader found, batched a few hundred milliseconds at a
  // time into `site_capture`, one batch per source. `media` remembers each
  // post's media under a key the page can recover from a post's DOM — an id,
  // an rkey, a shortcode — for the download button.
  const FLUSH_MS = 400
  const FLUSH_AT = 250

  function createBatcher() {
    const pending = { users: new Map(), posts: new Map(), source: '' }
    const media = new Map() // page key → { id, handle, media }
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

    // `found` is { users: Map, posts: Map }; `keyOf(post)` names the page
    // key its media is filed under.
    function add(source, found, keyOf) {
      if (!found.users.size && !found.posts.size) return
      // One batch per source, so a bookmarks page and a timeline that answer
      // in the same tick do not share a label.
      if (pending.source && pending.source !== source) flush()
      pending.source = source
      for (const [id, user] of found.users) pending.users.set(id, user)
      for (const [id, post] of found.posts) {
        pending.posts.set(id, post)
        if (post.media.length) media.set(keyOf ? keyOf(post) : id, { id, handle: post.authorHandle, media: post.media })
      }
      if (pending.users.size + pending.posts.size >= FLUSH_AT) flush()
      else if (!flushTimer) flushTimer = setTimeout(flush, FLUSH_MS)
    }

    return { add, flush, count: () => captured, media }
  }

  // ── The download button ───────────────────────────────────────────────────
  // On every post whose action bar is on screen and whose media the batcher
  // has seen — or, where the network's script says so, that shows a photo
  // the page drew. `config`: { article, barOf(article), postOf(article) →
  // { key, handle } | null, pageImages(article) → [] (optional), media }.
  const BUTTON_ATTR = 'data-twister-download'
  const DOWNLOAD_GLYPH =
    'M12 15.6l-4.5-4.5 1.4-1.4 2.1 2.1V3h2v8.8l2.1-2.1 1.4 1.4L12 15.6zM4 20v-5h2v3h12v-3h2v5H4z'

  function installDownloadButtons(config) {
    let enabled = true
    const pageImages = config.pageImages || (() => [])

    function decorate() {
      if (!enabled) return
      for (const article of document.querySelectorAll(config.article)) {
        const bar = config.barOf(article)
        if (!bar || bar.querySelector(`[${BUTTON_ATTR}]`)) continue
        const post = config.postOf(article)
        if (!post) continue
        const known = config.media.get(post.key)
        if (!known && !pageImages(article).length) continue
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
          const entry = config.media.get(post.key)
          invoke('site_download', {
            request: {
              postId: entry ? entry.id : post.key,
              handle: entry ? entry.handle : post.handle,
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

    return {
      setEnabled(on) {
        enabled = Boolean(on)
        if (!enabled) for (const b of document.querySelectorAll(`[${BUTTON_ATTR}]`)) b.remove()
        else decorate()
      },
    }
  }

  // ── The operations runner ─────────────────────────────────────────────────
  // Started by Rust (`ops.rs`) in the active tab, which is also the only
  // thing that reads what it reports. A scan (scroll a list to its end so
  // the capture hook sees all of it) is the same on every site. Follow,
  // unfollow and delete walk the page one viewport at a time — sites unmount
  // cells that leave the viewport, so jumping to the end would skip
  // everything between — and act through the network's own `actOnCell` and
  // `removeArticle`; compose is the network's own. Everything destructive is
  // a dry run unless told otherwise: it walks the page and reports what it
  // WOULD do.
  //
  // `config`: { siteName, userCell, article, column, empty, toast, lockout,
  // limits: { follow, unfollow, delete }, ownHandle(), handleOf(cell),
  // postOf(article), actOnCell(ctx, cell, kind), removeArticle(ctx, article),
  // compose(ctx, params) }.
  const DELAY = [2000, 5000]
  const IDLE_ROUNDS = 6
  const MAX_ROUNDS = 600
  const MAX_FAILURES = 2
  const CONTENT_TIMEOUT = 45000

  function createOps(config) {
    let current = null // { id, cancelled }

    const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms))
    const jitter = () => DELAY[0] + Math.random() * (DELAY[1] - DELAY[0])

    function report(progress, extra) {
      if (!current) return
      invoke('site_op_progress', Object.assign({ id: current.id, progress }, extra || {}))
    }

    class Stop extends Error {
      constructor(status, message) {
        super(message)
        this.status = status
      }
    }

    function check() {
      if (!current || current.cancelled) throw new Stop('cancelled', 'Stopped.')
      if (config.lockout && config.lockout.test(location.pathname)) {
        throw new Stop('failed', `${config.siteName} asked you to sign in again.`)
      }
    }

    function toastText() {
      if (!config.toast) return ''
      for (const toast of document.querySelectorAll(config.toast)) {
        const text = (toast.textContent || '').trim()
        if (text) return text
      }
      return ''
    }

    async function waitFor(selector, timeout, root) {
      const until = Date.now() + timeout
      while (Date.now() < until) {
        const found = (root || document).querySelector(selector)
        if (found) return found
        await sleep(100)
      }
      return null
    }

    // A site draws its shell long before its content. Nothing may start
    // until the column has cells, articles, or the site's own word that
    // there are none.
    async function waitForContent(selector) {
      const until = Date.now() + CONTENT_TIMEOUT
      const probe = config.empty ? `${selector}, ${config.empty}` : selector
      while (Date.now() < until) {
        check()
        const column = config.column ? document.querySelector(config.column) : document.body
        if (column && column.querySelector(probe)) return
        await sleep(250)
      }
      throw new Stop('failed', `${config.siteName} did not draw the page in time.`)
    }

    function scrollContainer() {
      const probe = document.querySelector(config.userCell) || document.querySelector(config.article)
      let node = probe && probe.parentElement
      while (node && node !== document.body) {
        const style = getComputedStyle(node)
        if ((style.overflowY === 'auto' || style.overflowY === 'scroll') && node.scrollHeight > node.clientHeight + 10) {
          return node
        }
        node = node.parentElement
      }
      return null
    }

    function scrollHeight(container) {
      return container ? container.scrollHeight : document.documentElement.scrollHeight
    }

    function scrollToEnd(container) {
      if (container) container.scrollTop = container.scrollHeight
      else window.scrollTo(0, document.documentElement.scrollHeight)
    }

    // A scroll round: to the bottom, then a pause for the site to fetch
    // more. Returns whether the page grew.
    async function scrollRound(container) {
      const before = scrollHeight(container)
      scrollToEnd(container)
      await sleep(900 + Math.random() * 600)
      return scrollHeight(container) > before
    }

    // A walk's scroll round: one viewport down, then a pause for the site to
    // draw. Returns whether the page moved or grew.
    async function scrollStep(container) {
      const before = container ? container.scrollTop : window.scrollY
      const height = scrollHeight(container)
      const step = (container ? container.clientHeight : window.innerHeight) * 0.8
      if (container) container.scrollTop = before + step
      else window.scrollBy(0, step)
      await sleep(700 + Math.random() * 500)
      const after = container ? container.scrollTop : window.scrollY
      return after !== before || scrollHeight(container) > height
    }

    const ctx = {
      invoke,
      sleep,
      jitter,
      report,
      Stop,
      check,
      toastText,
      waitFor,
      waitForContent,
      scrollContainer,
      scrollStep,
      siteName: config.siteName,
    }

    // ── Scan ──
    async function scan(params) {
      const limit = Number(params.limit) > 0 ? Number(params.limit) : Infinity
      const capture = window.__twisterCapture
      const start = capture ? capture.count() : 0
      await waitForContent(`${config.userCell}, ${config.article}`)
      let idle = 0
      let seen = 0
      for (let round = 0; round < MAX_ROUNDS; round += 1) {
        check()
        const grew = await scrollRound(scrollContainer())
        const now = capture ? capture.count() - start : 0
        if (now > seen) {
          seen = now
          idle = 0
        } else if (!grew) {
          idle += 1
        }
        report({ done: seen, message: `Captured ${seen} so far` })
        if (seen >= limit || idle >= IDLE_ROUNDS) break
      }
      if (capture) capture.flush()
      return { done: seen, message: `Captured ${seen} from ${location.pathname}` }
    }

    // ── Follow and unfollow ──
    async function people(kind, params, dryRun) {
      const wanted = new Set((params.handles || []).map((h) => String(h).toLowerCase()))
      wanted.delete((config.ownHandle() || '').toLowerCase())
      const cap = Math.min(Number(params.maxPerRun) > 0 ? Number(params.maxPerRun) : config.limits[kind], config.limits[kind])
      const total = wanted.size
      const processed = new Set()
      let done = 0
      let skipped = 0
      let failed = 0
      let failures = 0
      let idle = 0
      report({ total, message: dryRun ? 'Dry run: looking for them on this page' : 'Working through the page' })
      await waitForContent(config.userCell)
      for (let round = 0; round < MAX_ROUNDS && processed.size < total; round += 1) {
        check()
        let acted = false
        for (const cell of document.querySelectorAll(config.userCell)) {
          const handle = config.handleOf(cell)
          if (!handle || !wanted.has(handle) || processed.has(handle)) continue
          processed.add(handle)
          check()
          if (done >= cap) {
            skipped += 1
            report({ done, skipped, failed, message: `Stopped at the per-run cap of ${cap}` })
            continue
          }
          if (dryRun) {
            done += 1
            report({ done, skipped, failed, message: `Would ${kind} @${handle}` })
            continue
          }
          cell.scrollIntoView({ block: 'center' })
          await sleep(300)
          const result = await config.actOnCell(ctx, cell, kind)
          if (result.ok) {
            done += 1
            failures = 0
            report({ done, skipped, failed, message: `${result.already ? 'Already' : 'Did'} ${kind} @${handle}` })
          } else {
            failed += 1
            failures += 1
            report({ done, skipped, failed, message: result.toast ? `${config.siteName} said: ${result.toast}` : `Could not ${kind} @${handle}` })
            if (result.toast) throw new Stop('failed', `${config.siteName} said: ${result.toast}`)
            if (failures >= MAX_FAILURES) throw new Stop('failed', `Two failures in a row; stopping before ${config.siteName} does.`)
          }
          acted = true
          await sleep(jitter())
        }
        if (processed.size >= total) break
        const moved = await scrollStep(scrollContainer())
        idle = moved || acted ? 0 : idle + 1
        if (idle >= IDLE_ROUNDS) break
      }
      const missing = total - processed.size
      return {
        done,
        skipped: skipped + missing,
        failed,
        message: missing ? `${missing} of them were not on this page` : `${done} ${kind}${dryRun ? ' (dry run)' : ''}`,
      }
    }

    // ── Delete ──
    async function remove(params, dryRun) {
      const wanted = new Set((params.ids || []).map(String))
      const total = wanted.size
      const processed = new Set()
      let done = 0
      let skipped = 0
      let failed = 0
      let failures = 0
      let idle = 0
      report({ total, message: dryRun ? 'Dry run: looking for them on this page' : 'Working through the page' })
      await waitForContent(config.article)
      for (let round = 0; round < MAX_ROUNDS && processed.size < total; round += 1) {
        check()
        let acted = false
        for (const article of document.querySelectorAll(config.article)) {
          const id = config.postOf(article)
          if (!id || !wanted.has(id) || processed.has(id)) continue
          processed.add(id)
          check()
          if (done >= config.limits.delete) {
            skipped += 1
            continue
          }
          if (dryRun) {
            done += 1
            report({ done, skipped, failed, message: `Would delete ${id}` })
            continue
          }
          article.scrollIntoView({ block: 'center' })
          await sleep(300)
          const result = await config.removeArticle(ctx, article)
          if (result.ok) {
            done += 1
            failures = 0
            report({ done, skipped, failed, message: `Deleted ${id}` }, { removed: [id] })
          } else if (result.notOurs) {
            skipped += 1
            report({ done, skipped, failed, message: `${id} is not yours to delete` })
          } else {
            failed += 1
            failures += 1
            report({ done, skipped, failed, message: result.toast ? `${config.siteName} said: ${result.toast}` : `Could not delete ${id}` })
            if (result.toast) throw new Stop('failed', `${config.siteName} said: ${result.toast}`)
            if (failures >= MAX_FAILURES) throw new Stop('failed', `Two failures in a row; stopping before ${config.siteName} does.`)
          }
          acted = true
          await sleep(jitter())
        }
        if (processed.size >= total) break
        const moved = await scrollStep(scrollContainer())
        idle = moved || acted ? 0 : idle + 1
        if (idle >= IDLE_ROUNDS) break
      }
      const missing = total - processed.size
      return {
        done,
        skipped: skipped + missing,
        failed,
        message: missing ? `${missing} of them were not on this page` : `${done} deleted${dryRun ? ' (dry run)' : ''}`,
      }
    }

    const unsupported = (kind) => async () => {
      throw new Stop('failed', `Twister does not ${kind} on ${config.siteName}.`)
    }

    const RUNNERS = {
      scan: (params) => scan(params),
      follow: config.actOnCell ? (params, dryRun) => people('follow', params, dryRun) : unsupported('follow'),
      unfollow: config.actOnCell ? (params, dryRun) => people('unfollow', params, dryRun) : unsupported('unfollow'),
      delete: config.removeArticle ? (params, dryRun) => remove(params, dryRun) : unsupported('delete'),
      compose: config.compose ? (params) => config.compose(ctx, params) : unsupported('post'),
    }

    async function run(id, kind, params, dryRun) {
      if (current) {
        invoke('site_op_progress', { id, progress: { status: 'failed', message: 'Another operation is running in this page.' } })
        return
      }
      current = { id, cancelled: false }
      const runner = RUNNERS[kind]
      try {
        if (!runner) throw new Stop('failed', `Unknown operation ${kind}.`)
        check()
        const outcome = await runner(params || {}, Boolean(dryRun))
        report(Object.assign({ status: 'done' }, outcome))
      } catch (err) {
        const status = err instanceof Stop ? err.status : 'failed'
        report({ status, message: err && err.message ? String(err.message).slice(0, 300) : 'Failed' })
      } finally {
        current = null
      }
    }

    function cancel() {
      if (current) current.cancelled = true
    }

    window.__twisterOps = { run, cancel }
  }

  // ── Small shared readers ──────────────────────────────────────────────────
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

  // Any date a site writes → RFC 3339, or empty.
  function iso(value) {
    if (typeof value === 'number' && Number.isFinite(value)) {
      // Unix seconds, as Meta writes them.
      return new Date(value * 1000).toISOString().replace(/\.\d{3}Z$/, 'Z')
    }
    if (typeof value !== 'string' || !value) return ''
    const ms = Date.parse(value)
    return Number.isFinite(ms) ? new Date(ms).toISOString().replace(/\.\d{3}Z$/, 'Z') : ''
  }

  // A bounded walk over a payload, calling `visit(value)` on every object.
  // Sites nest deeply but never past MAX_DEPTH, and a response that would
  // take more than MAX_NODES is a timeline the store gets in pieces anyway.
  const MAX_DEPTH = 40
  const MAX_NODES = 60000
  function walk(payload, visit) {
    let nodes = 0
    const seen = new WeakSet()
    const step = (value, depth) => {
      if (!value || typeof value !== 'object' || depth > MAX_DEPTH || nodes > MAX_NODES) return
      if (seen.has(value)) return
      seen.add(value)
      nodes += 1
      if (Array.isArray(value)) {
        for (const item of value) step(item, depth + 1)
        return
      }
      visit(value)
      for (const key in value) step(value[key], depth + 1)
    }
    step(payload, 0)
  }

  // One observer for everything DOM-shaped, coalesced to a task: a site
  // mutates constantly while a timeline streams in. A task rather than a
  // frame, because a tab behind another gets no frames — and the hidden
  // tabs are exactly where the inline cache has to be read.
  function observe(callback, options) {
    let scheduled = false
    const observer = new MutationObserver(() => {
      if (scheduled) return
      scheduled = true
      setTimeout(() => {
        scheduled = false
        callback()
      }, 0)
    })
    observer.observe(document.documentElement, options || { childList: true, subtree: true })
    return observer
  }

  // Types text into a site's editor the way a person types: focus the box
  // and insert, which is what every contenteditable editor listens for.
  function insertText(box, text) {
    box.focus()
    const inserted = document.execCommand('insertText', false, text)
    if (!inserted) {
      box.dispatchEvent(new InputEvent('beforeinput', { inputType: 'insertText', data: text, bubbles: true }))
      box.textContent = text
      box.dispatchEvent(new InputEvent('input', { inputType: 'insertText', data: text, bubbles: true }))
    }
  }

  window.__twisterCommon = {
    invoke,
    installSheet,
    onResponse,
    createBatcher,
    installDownloadButtons,
    createOps,
    observe,
    insertText,
    str,
    num,
    flag,
    iso,
    walk,
  }
})()
