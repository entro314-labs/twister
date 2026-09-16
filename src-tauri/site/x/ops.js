// Twister's operations on X. Injected at document start into every X tab,
// after common.js, which holds the runner; started by Rust (`ops.rs`) in
// the active tab.
//
// What is X's here: the selectors, how a follow or unfollow is done on a
// person cell, how a post is deleted (or a repost undone, or a like taken
// back) through X's own menu, and how a thread goes through X's composer.
// All of it clicks what X drew, waits for X to confirm by redrawing, and
// stops the moment X shows a toast — which is how X says "slow down" in
// every language.
(() => {
  'use strict'
  if (window.__twisterOps) return
  const common = window.__twisterCommon
  if (!common) return

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

  function ownHandle() {
    const link = document.querySelector('a[data-testid="AppTabBar_Profile_Link"]')
    return link ? (link.getAttribute('href') || '').replace(/^\//, '') : ''
  }

  // ── Follow and unfollow ───────────────────────────────────────────────────

  async function verify(ctx, cell, wanted, unwanted) {
    const until = Date.now() + 4000
    while (Date.now() < until) {
      const toast = ctx.toastText()
      if (toast) return { ok: false, toast }
      if (!cell.isConnected) return { ok: true }
      if (cell.querySelector(wanted) && !cell.querySelector(unwanted)) return { ok: true }
      await ctx.sleep(150)
    }
    return { ok: false }
  }

  async function actOnCell(ctx, cell, kind) {
    if (kind === 'unfollow') {
      const button = cell.querySelector(SEL.unfollow)
      if (!button) return cell.querySelector(SEL.follow) ? { ok: true, already: true } : { ok: false }
      button.click()
      const confirm = await ctx.waitFor(SEL.confirm, 4000)
      if (!confirm) return { ok: false }
      confirm.click()
      return verify(ctx, cell, SEL.follow, SEL.unfollow)
    }
    const button = cell.querySelector(SEL.follow)
    if (!button) return cell.querySelector(SEL.unfollow) ? { ok: true, already: true } : { ok: false }
    button.click()
    return verify(ctx, cell, SEL.unfollow, SEL.follow)
  }

  // ── Delete ────────────────────────────────────────────────────────────────

  async function closeMenus(ctx) {
    document.body.click()
    await ctx.sleep(200)
  }

  async function removeArticle(ctx, article) {
    if (/\/likes\/?$/.test(location.pathname)) {
      const unlike = article.querySelector(SEL.unlike)
      if (!unlike) return { ok: false }
      unlike.click()
      await ctx.sleep(400)
      return { ok: true }
    }
    const unretweet = article.querySelector(SEL.unretweet)
    if (unretweet) {
      unretweet.click()
      const confirm = await ctx.waitFor(SEL.unretweetConfirm, 3000)
      if (!confirm) return { ok: false }
      confirm.click()
      await ctx.sleep(600)
      return { ok: true }
    }
    const caret = article.querySelector(SEL.caret)
    if (!caret) return { ok: false }
    caret.click()
    await ctx.waitFor('[role="menuitem"]', 3000)
    let item = null
    for (const candidate of document.querySelectorAll('[role="menuitem"]')) {
      const path = candidate.querySelector('svg path')
      if (path && (path.getAttribute('d') || '').startsWith(TRASH)) {
        item = candidate
        break
      }
    }
    if (!item) {
      await closeMenus(ctx)
      return { ok: false, notOurs: true }
    }
    item.click()
    const confirm = await ctx.waitFor(SEL.confirm, 4000)
    if (!confirm) {
      await closeMenus(ctx)
      return { ok: false }
    }
    confirm.click()
    const until = Date.now() + 4000
    while (Date.now() < until) {
      const toast = ctx.toastText()
      if (toast) return { ok: false, toast }
      if (!article.isConnected) return { ok: true }
      await ctx.sleep(150)
    }
    return { ok: true }
  }

  // ── Compose ───────────────────────────────────────────────────────────────
  // Through X's own composer, typed the way a person types.

  async function compose(ctx, params) {
    const parts = (params.parts || []).map(String)
    if (!parts.length) throw new ctx.Stop('failed', 'Nothing to post.')
    ctx.report({ total: parts.length, message: 'Opening the composer' })
    if (!(window.__twister && window.__twister.go('/compose/post'))) {
      throw new ctx.Stop('failed', 'Could not open the composer: X has not drawn its navigation yet.')
    }
    const first = await ctx.waitFor(SEL.textarea, 8000)
    if (!first) throw new ctx.Stop('failed', 'The composer did not open.')
    await ctx.sleep(400)
    for (let i = 0; i < parts.length; i += 1) {
      ctx.check()
      let box = first
      if (i > 0) {
        const add = await ctx.waitFor(SEL.addPart, 4000)
        if (!add) throw new ctx.Stop('failed', 'Could not add a post to the thread.')
        add.click()
        box = await ctx.waitFor(`[data-testid="tweetTextarea_${i}"]`, 4000)
        if (!box) throw new ctx.Stop('failed', 'The next post in the thread did not appear.')
        await ctx.sleep(300)
      }
      common.insertText(box, parts[i])
      await ctx.sleep(300)
      ctx.report({ done: i + 1, message: `Typed ${i + 1} of ${parts.length}` })
    }
    ctx.check()
    const send = await ctx.waitFor(SEL.send, 4000)
    if (!send || send.getAttribute('aria-disabled') === 'true' || send.disabled) {
      throw new ctx.Stop('failed', 'X will not accept this post as typed. It is still in the composer.')
    }
    send.click()
    const until = Date.now() + 12000
    while (Date.now() < until) {
      const toast = ctx.toastText()
      if (toast && !/sent|posted|your post/i.test(toast)) throw new ctx.Stop('failed', `X said: ${toast}`)
      if (!document.querySelector(SEL.textarea)) return { done: parts.length, message: 'Posted' }
      await ctx.sleep(200)
    }
    throw new ctx.Stop('failed', 'X did not confirm the post went out. Check the composer.')
  }

  common.createOps({
    siteName: 'X',
    userCell: SEL.userCell,
    article: SEL.article,
    column: '[data-testid="primaryColumn"]',
    empty: '[data-testid="emptyState"], [data-testid="empty_state_header_text"]',
    toast: SEL.toast,
    lockout: /^\/(?:i\/flow\/login|account\/access|i\/flow\/consent)/,
    limits: { follow: 50, unfollow: 100, delete: 200 },
    ownHandle,
    handleOf,
    postOf,
    actOnCell,
    removeArticle,
    compose,
  })
})()
