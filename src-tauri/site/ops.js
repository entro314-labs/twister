// Twister's operations. Injected at document start into every tab; started
// by Rust (`ops.rs`) in the active one, which is also the only thing that
// reads what they report.
//
// Four things run here: a scan (scroll a list to its end so the capture
// hook sees all of it), follow and unfollow (act on people by handle, on
// the list page that holds them), delete (posts by id, on your profile) and
// compose (a thread through X's own composer). All of them click what X
// drew, wait for X to confirm by redrawing, and stop the moment X shows a
// toast — which is how X says "slow down" in every language.
//
// Every destructive run is a dry run unless told otherwise: it walks the
// page and reports what it WOULD do.
(() => {
  'use strict'
  if (window.__twisterOps) return

  const internals = window.__TAURI_INTERNALS__
  const invoke = (command, args) =>
    internals ? internals.invoke(command, args).catch(() => undefined) : Promise.resolve(undefined)

  const SEL = {
    userCell: '[data-testid="UserCell"]',
    follow: '[data-testid$="-follow"]',
    unfollow: '[data-testid$="-unfollow"]',
    confirm: '[data-testid="confirmationSheetConfirm"]',
    toast: '[data-testid="toast"]',
    article: 'article[data-testid="tweet"]',
    caret: '[data-testid="caret"]',
    unretweet: '[data-testid="unretweet"]',
    unretweetConfirm: '[data-testid="unretweetConfirm"]',
    unlike: '[data-testid="unlike"]',
    textarea: '[data-testid="tweetTextarea_0"]',
    addPart: '[data-testid="addButton"]',
    send: '[data-testid="tweetButton"]',
  }
  // X's trash-can glyph, which is how "Delete" is found whatever the language.
  const TRASH = 'M16 6V4.5C16 3.12 14.88 2 13.5 2h-3C9.11 2 8 3.12 8 4.5V6H3v2h1.06'
  const LOCKOUT = /^\/(?:i\/flow\/login|account\/access|i\/flow\/consent)/

  const LIMITS = {
    follow: 50,
    unfollow: 100,
    delete: 200,
  }
  const DELAY = [2000, 5000]
  const IDLE_ROUNDS = 6
  const MAX_ROUNDS = 600
  const MAX_FAILURES = 2

  // ── Plumbing ──────────────────────────────────────────────────────────────

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
    if (LOCKOUT.test(location.pathname)) throw new Stop('failed', 'X asked you to sign in again.')
  }

  function toastText() {
    for (const toast of document.querySelectorAll(SEL.toast)) {
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

  // X draws its shell long before its content, and a cold load sits on the
  // splash for a while. Nothing here may start until the column has cells,
  // articles, or X's own word that there are none.
  const EMPTY = '[data-testid="emptyState"], [data-testid="empty_state_header_text"]'
  async function waitForContent(selector) {
    const until = Date.now() + CONTENT_TIMEOUT
    while (Date.now() < until) {
      check()
      const column = document.querySelector('[data-testid="primaryColumn"]')
      if (column && column.querySelector(`${selector}, ${EMPTY}`)) return
      await sleep(250)
    }
    throw new Stop('failed', 'X did not draw the page in time.')
  }
  const CONTENT_TIMEOUT = 45000

  function scrollContainer() {
    const probe = document.querySelector(SEL.userCell) || document.querySelector(SEL.article)
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

  // A scroll round: to the bottom, then a pause for X to fetch more. Returns
  // whether the page grew.
  async function scrollRound(container) {
    const before = scrollHeight(container)
    scrollToEnd(container)
    await sleep(900 + Math.random() * 600)
    return scrollHeight(container) > before
  }

  // A walk's scroll round: one viewport down, then a pause for X to draw. X
  // unmounts cells that leave the viewport, so jumping to the end would skip
  // everything in between. Returns whether the page moved or grew.
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

  function handleOf(cell) {
    for (const link of cell.querySelectorAll('a[role="link"][href^="/"]')) {
      const match = /^\/([A-Za-z0-9_]{1,15})(?:[/?#]|$)/.exec(link.getAttribute('href') || '')
      if (match && !/^(?:i|home|explore|search|settings|messages)$/.test(match[1])) return match[1].toLowerCase()
    }
    return ''
  }

  function postOf(article) {
    for (const link of article.querySelectorAll('a[href*="/status/"]')) {
      const match = /^\/([A-Za-z0-9_]{1,15})\/status\/(\d{1,25})(?:[/?#]|$)/.exec(link.getAttribute('href') || '')
      if (match && link.querySelector('time')) return match[2]
    }
    return ''
  }

  // ── Scan ──────────────────────────────────────────────────────────────────

  async function scan(params) {
    const limit = Number(params.limit) > 0 ? Number(params.limit) : Infinity
    const capture = window.__twisterCapture
    const start = capture ? capture.count() : 0
    await waitForContent(`${SEL.userCell}, ${SEL.article}`)
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

  // ── Follow and unfollow ───────────────────────────────────────────────────

  async function verify(cell, wanted, unwanted) {
    const until = Date.now() + 4000
    while (Date.now() < until) {
      const toast = toastText()
      if (toast) return { ok: false, toast }
      if (!cell.isConnected) return { ok: true }
      if (cell.querySelector(wanted) && !cell.querySelector(unwanted)) return { ok: true }
      await sleep(150)
    }
    return { ok: false }
  }

  async function actOnCell(cell, kind) {
    if (kind === 'unfollow') {
      const button = cell.querySelector(SEL.unfollow)
      if (!button) return cell.querySelector(SEL.follow) ? { ok: true, already: true } : { ok: false }
      button.click()
      const confirm = await waitFor(SEL.confirm, 4000)
      if (!confirm) return { ok: false }
      confirm.click()
      return verify(cell, SEL.follow, SEL.unfollow)
    }
    const button = cell.querySelector(SEL.follow)
    if (!button) return cell.querySelector(SEL.unfollow) ? { ok: true, already: true } : { ok: false }
    button.click()
    return verify(cell, SEL.unfollow, SEL.follow)
  }

  async function people(kind, params, dryRun) {
    const wanted = new Set((params.handles || []).map((h) => String(h).toLowerCase()))
    const own = (document.querySelector('a[data-testid="AppTabBar_Profile_Link"]') || {}).getAttribute
      ? (document.querySelector('a[data-testid="AppTabBar_Profile_Link"]').getAttribute('href') || '').replace(/^\//, '').toLowerCase()
      : ''
    wanted.delete(own)
    const cap = Math.min(Number(params.maxPerRun) > 0 ? Number(params.maxPerRun) : LIMITS[kind], LIMITS[kind])
    const total = wanted.size
    const processed = new Set()
    let done = 0
    let skipped = 0
    let failed = 0
    let failures = 0
    let idle = 0
    report({ total, message: dryRun ? 'Dry run: looking for them on this page' : 'Working through the page' })
    await waitForContent(SEL.userCell)
    for (let round = 0; round < MAX_ROUNDS && processed.size < total; round += 1) {
      check()
      let acted = false
      for (const cell of document.querySelectorAll(SEL.userCell)) {
        const handle = handleOf(cell)
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
        const result = await actOnCell(cell, kind)
        if (result.ok) {
          done += 1
          failures = 0
          report({ done, skipped, failed, message: `${result.already ? 'Already' : 'Did'} ${kind} @${handle}` })
        } else {
          failed += 1
          failures += 1
          report({ done, skipped, failed, message: result.toast ? `X said: ${result.toast}` : `Could not ${kind} @${handle}` })
          if (result.toast) throw new Stop('failed', `X said: ${result.toast}`)
          if (failures >= MAX_FAILURES) throw new Stop('failed', 'Two failures in a row; stopping before X does.')
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

  // ── Delete ────────────────────────────────────────────────────────────────

  async function closeMenus() {
    document.body.click()
    await sleep(200)
  }

  async function removeArticle(article) {
    if (/\/likes\/?$/.test(location.pathname)) {
      const unlike = article.querySelector(SEL.unlike)
      if (!unlike) return { ok: false }
      unlike.click()
      await sleep(400)
      return { ok: true }
    }
    const unretweet = article.querySelector(SEL.unretweet)
    if (unretweet) {
      unretweet.click()
      const confirm = await waitFor(SEL.unretweetConfirm, 3000)
      if (!confirm) return { ok: false }
      confirm.click()
      await sleep(600)
      return { ok: true }
    }
    const caret = article.querySelector(SEL.caret)
    if (!caret) return { ok: false }
    caret.click()
    await waitFor('[role="menuitem"]', 3000)
    let item = null
    for (const candidate of document.querySelectorAll('[role="menuitem"]')) {
      const path = candidate.querySelector('svg path')
      if (path && (path.getAttribute('d') || '').startsWith(TRASH)) {
        item = candidate
        break
      }
    }
    if (!item) {
      await closeMenus()
      return { ok: false, notOurs: true }
    }
    item.click()
    const confirm = await waitFor(SEL.confirm, 4000)
    if (!confirm) {
      await closeMenus()
      return { ok: false }
    }
    confirm.click()
    const until = Date.now() + 4000
    while (Date.now() < until) {
      const toast = toastText()
      if (toast) return { ok: false, toast }
      if (!article.isConnected) return { ok: true }
      await sleep(150)
    }
    return { ok: true }
  }

  async function remove(params, dryRun) {
    const wanted = new Set((params.ids || []).map(String))
    const total = wanted.size
    const processed = new Set()
    const removed = []
    let done = 0
    let skipped = 0
    let failed = 0
    let failures = 0
    let idle = 0
    report({ total, message: dryRun ? 'Dry run: looking for them on this page' : 'Working through the page' })
    await waitForContent(SEL.article)
    for (let round = 0; round < MAX_ROUNDS && processed.size < total; round += 1) {
      check()
      let acted = false
      for (const article of document.querySelectorAll(SEL.article)) {
        const id = postOf(article)
        if (!id || !wanted.has(id) || processed.has(id)) continue
        processed.add(id)
        check()
        if (done >= LIMITS.delete) {
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
        const result = await removeArticle(article)
        if (result.ok) {
          done += 1
          failures = 0
          removed.push(id)
          report({ done, skipped, failed, message: `Deleted ${id}` }, { removed: [id] })
        } else if (result.notOurs) {
          skipped += 1
          report({ done, skipped, failed, message: `${id} is not yours to delete` })
        } else {
          failed += 1
          failures += 1
          report({ done, skipped, failed, message: result.toast ? `X said: ${result.toast}` : `Could not delete ${id}` })
          if (result.toast) throw new Stop('failed', `X said: ${result.toast}`)
          if (failures >= MAX_FAILURES) throw new Stop('failed', 'Two failures in a row; stopping before X does.')
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

  // ── Compose ───────────────────────────────────────────────────────────────
  // Through X's own composer, typed the way a person types: focus the box
  // and insert text, which is what X's editor listens for.

  function insert(box, text) {
    box.focus()
    const inserted = document.execCommand('insertText', false, text)
    if (!inserted) {
      box.dispatchEvent(new InputEvent('beforeinput', { inputType: 'insertText', data: text, bubbles: true }))
      box.textContent = text
      box.dispatchEvent(new InputEvent('input', { inputType: 'insertText', data: text, bubbles: true }))
    }
  }

  async function compose(params) {
    const parts = (params.parts || []).map(String)
    if (!parts.length) throw new Stop('failed', 'Nothing to post.')
    report({ total: parts.length, message: 'Opening the composer' })
    if (!(window.__twister && window.__twister.go('/compose/post'))) {
      throw new Stop('failed', 'Could not open the composer: X has not drawn its navigation yet.')
    }
    const first = await waitFor(SEL.textarea, 8000)
    if (!first) throw new Stop('failed', 'The composer did not open.')
    await sleep(400)
    for (let i = 0; i < parts.length; i += 1) {
      check()
      let box = first
      if (i > 0) {
        const add = await waitFor(SEL.addPart, 4000)
        if (!add) throw new Stop('failed', 'Could not add a post to the thread.')
        add.click()
        box = await waitFor(`[data-testid="tweetTextarea_${i}"]`, 4000)
        if (!box) throw new Stop('failed', 'The next post in the thread did not appear.')
        await sleep(300)
      }
      insert(box, parts[i])
      await sleep(300)
      report({ done: i + 1, message: `Typed ${i + 1} of ${parts.length}` })
    }
    check()
    const send = await waitFor(SEL.send, 4000)
    if (!send || send.getAttribute('aria-disabled') === 'true' || send.disabled) {
      throw new Stop('failed', 'X will not accept this post as typed. It is still in the composer.')
    }
    send.click()
    const until = Date.now() + 12000
    while (Date.now() < until) {
      const toast = toastText()
      if (toast && !/sent|posted|your post/i.test(toast)) throw new Stop('failed', `X said: ${toast}`)
      if (!document.querySelector(SEL.textarea)) return { done: parts.length, message: 'Posted' }
      await sleep(200)
    }
    throw new Stop('failed', 'X did not confirm the post went out. Check the composer.')
  }

  // ── Entry ─────────────────────────────────────────────────────────────────

  const RUNNERS = {
    scan: (params) => scan(params),
    follow: (params, dryRun) => people('follow', params, dryRun),
    unfollow: (params, dryRun) => people('unfollow', params, dryRun),
    delete: (params, dryRun) => remove(params, dryRun),
    compose: (params) => compose(params),
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
})()
