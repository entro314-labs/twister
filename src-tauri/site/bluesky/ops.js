// Twister's operations on Bluesky. Injected at document start into every
// Bluesky tab, after common.js, which holds the runner; started by Rust
// (`ops.rs`) in the active tab.
//
// What is Bluesky's here: the selectors, how a follow or unfollow is done
// on a profile card, how a post is deleted through its own menu, and how a
// single post goes through the composer. Test ids seen on bsky.app on
// 2026-09-16 signed out (`followBtn`, `postDropdownBtn`, `feedItem-by-*`);
// the signed-in ones — the follows list's cards, the dropdown's delete
// item, the confirm button, the composer — are the app's documented names
// and have NOT been exercised live. Caps are lower than X's for that
// reason, and every run is a dry run unless told otherwise.
(() => {
  'use strict'
  if (window.__twisterOps) return
  const common = window.__twisterCommon
  if (!common) return

  const SEL = {
    // A person in a follows or followers list: the card's own link carries
    // the handle in its test id and its href.
    userCell: '[data-testid^="profileCard-"]',
    follow: '[data-testid="followBtn"]',
    unfollow: '[data-testid="unfollowBtn"]',
    confirm: '[data-testid="confirmBtn"]',
    article: '[data-testid^="feedItem-by-"], [data-testid^="postThreadItem-by-"]',
    dropdown: '[data-testid="postDropdownBtn"]',
    deleteItem: '[data-testid="postDropdownDeleteBtn"]',
    composer: '[data-testid="composerTextInput"]',
    send: '[data-testid="composerPostButton"]',
  }

  function handleOf(cell) {
    const match = /^\/profile\/([^/?#]+)/.exec(cell.getAttribute('href') || '')
    return match && !match[1].startsWith('did:') ? match[1].toLowerCase() : ''
  }

  // The store names a post by its at-URI; the page names it by its rkey.
  function postOf(article) {
    for (const link of article.querySelectorAll('a[href*="/post/"]')) {
      const match = /^\/profile\/[^/]+\/post\/([a-z2-7]{1,20})(?:[/?#]|$)/.exec(link.getAttribute('href') || '')
      if (match) return window.__twisterCapture ? window.__twisterCapture.uriOf(match[1]) : ''
    }
    return ''
  }

  function ownHandle() {
    try {
      const parsed = JSON.parse(localStorage.getItem('BSKY_STORAGE') || 'null')
      const account = parsed && parsed.session && parsed.session.currentAccount
      return account && typeof account.handle === 'string' ? account.handle : ''
    } catch {
      return ''
    }
  }

  // The card's link is the cell; the buttons sit beside it in the row.
  function rowOf(cell) {
    let node = cell
    for (let i = 0; i < 5 && node; i += 1) {
      if (node.querySelector(SEL.follow) || node.querySelector(SEL.unfollow)) return node
      node = node.parentElement
    }
    return cell
  }

  async function verify(ctx, row, wanted, unwanted) {
    const until = Date.now() + 4000
    while (Date.now() < until) {
      if (!row.isConnected) return { ok: true }
      if (row.querySelector(wanted) && !row.querySelector(unwanted)) return { ok: true }
      await ctx.sleep(150)
    }
    return { ok: false }
  }

  async function actOnCell(ctx, cell, kind) {
    const row = rowOf(cell)
    if (kind === 'unfollow') {
      const button = row.querySelector(SEL.unfollow)
      if (!button) return row.querySelector(SEL.follow) ? { ok: true, already: true } : { ok: false }
      button.click()
      return verify(ctx, row, SEL.follow, SEL.unfollow)
    }
    const button = row.querySelector(SEL.follow)
    if (!button) return row.querySelector(SEL.unfollow) ? { ok: true, already: true } : { ok: false }
    button.click()
    return verify(ctx, row, SEL.unfollow, SEL.follow)
  }

  async function removeArticle(ctx, article) {
    const dropdown = article.querySelector(SEL.dropdown)
    if (!dropdown) return { ok: false }
    dropdown.click()
    const item = await ctx.waitFor(SEL.deleteItem, 3000)
    if (!item) {
      document.body.click()
      await ctx.sleep(200)
      return { ok: false, notOurs: true }
    }
    item.click()
    const confirm = await ctx.waitFor(SEL.confirm, 4000)
    if (!confirm) {
      document.body.click()
      await ctx.sleep(200)
      return { ok: false }
    }
    confirm.click()
    const until = Date.now() + 4000
    while (Date.now() < until) {
      if (!article.isConnected) return { ok: true }
      await ctx.sleep(150)
    }
    return { ok: true }
  }

  // One post, through the app's own composer — opened by the intent route
  // it publishes for exactly this, with the text carried in the URL. A
  // thread would need the composer's add-post control, which has not been
  // seen live; it is refused rather than guessed at.
  async function compose(ctx, params) {
    const parts = (params.parts || []).map(String)
    if (!parts.length) throw new ctx.Stop('failed', 'Nothing to post.')
    if (parts.length > 1) {
      throw new ctx.Stop('failed', 'Twister posts one post at a time on Bluesky; threads are not supported there yet.')
    }
    ctx.report({ total: 1, message: 'Opening the composer' })
    const path = '/intent/compose?text=' + encodeURIComponent(parts[0])
    if (!(window.__twister && window.__twister.go(path))) location.assign(path)
    const box = await ctx.waitFor(SEL.composer, 10000)
    if (!box) throw new ctx.Stop('failed', 'The composer did not open.')
    await ctx.sleep(600)
    if (!(box.textContent || box.value || '').trim()) common.insertText(box, parts[0])
    await ctx.sleep(300)
    ctx.check()
    const send = await ctx.waitFor(SEL.send, 4000)
    if (!send || send.getAttribute('aria-disabled') === 'true' || send.disabled) {
      throw new ctx.Stop('failed', 'Bluesky will not accept this post as typed. It is still in the composer.')
    }
    send.click()
    const until = Date.now() + 12000
    while (Date.now() < until) {
      if (!document.querySelector(SEL.composer)) return { done: 1, message: 'Posted' }
      await ctx.sleep(200)
    }
    throw new ctx.Stop('failed', 'Bluesky did not confirm the post went out. Check the composer.')
  }

  common.createOps({
    siteName: 'Bluesky',
    userCell: SEL.userCell,
    article: SEL.article,
    column: null,
    empty: null,
    toast: null,
    lockout: null,
    limits: { follow: 30, unfollow: 60, delete: 100 },
    ownHandle,
    handleOf,
    postOf,
    actOnCell,
    removeArticle,
    compose,
  })
})()
