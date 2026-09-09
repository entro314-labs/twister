import './tooltip.css'

import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'

/**
 * The tooltip page. It lives in its own tiny window (see `src-tauri/src/tooltip.rs`), receives
 * content from the shell by event, renders it, measures itself, and tells Rust the size — only then
 * is the window placed and shown. Deliberately no framework: it is one element.
 */
interface Content {
  label: string
  shortcut: string | null
  theme: 'light' | 'dark'
}

const tip = document.getElementById('tip')
const label = document.getElementById('label')
const kbd = document.getElementById('kbd')
if (!tip || !label || !kbd) throw new Error('tooltip.html is missing its elements')

let generation = 0

void listen<Content>('twister://tooltip', ({ payload }) => {
  generation += 1
  const mine = generation
  document.documentElement.classList.toggle('dark', payload.theme === 'dark')
  document.documentElement.style.colorScheme = payload.theme
  label.textContent = payload.label
  kbd.textContent = payload.shortcut ?? ''
  kbd.hidden = !payload.shortcut
  tip.hidden = false
  // Restart the entrance so a tooltip that moves between anchors still eases in.
  tip.classList.remove('tip-in')
  void tip.offsetWidth
  tip.classList.add('tip-in')

  requestAnimationFrame(() => {
    if (mine !== generation) return
    const rect = tip.getBoundingClientRect()
    void invoke('tooltip_ready', {
      width: Math.ceil(rect.width) + 2,
      height: Math.ceil(rect.height) + 2,
    }).catch(() => {})
  })
})
