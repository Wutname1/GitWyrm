import { useEffect, useRef, useState } from 'react'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { commands } from '@/lib/bindings'

const inTauri = '__TAURI_INTERNALS__' in window

/**
 * Make a custom-drawn maximize button behave like the real one, so hovering it
 * opens the Windows Snap Layouts flyout.
 *
 * Windows only offers that flyout when the window answers `WM_NCHITTEST` with
 * `HTMAXBUTTON`, and a borderless window whose caption is HTML always answers
 * "client area" instead. The backend subclasses the window to answer correctly,
 * but it needs to be told where the button is -- hence the measure-and-report
 * below, repeated whenever the title bar reflows (a tab opens, the window
 * resizes, the display scale changes).
 *
 * Claiming those pixels as non-client means the webview stops receiving mouse
 * events there, so the button would look dead: no hover tint, no click. The
 * backend relays both back as events, and this hook turns them into the `hovered`
 * flag and the `onActivate` call the button would otherwise get from the DOM.
 *
 * Returns the ref to attach to the button and whether Windows currently
 * considers the cursor to be over it. Off Windows, and outside Tauri, nothing is
 * reported and `hovered` stays false -- CSS `:hover` still works there, because
 * the pixels were never claimed.
 */
export function useSnapLayouts(onActivate: () => void): {
  ref: React.RefObject<HTMLButtonElement | null>
  hovered: boolean
} {
  const ref = useRef<HTMLButtonElement>(null)
  const [hovered, setHovered] = useState(false)

  // Held in a ref so a new callback identity does not tear down the event
  // listeners: they are set up once and read whatever the latest handler is.
  const activate = useRef(onActivate)
  activate.current = onActivate

  useEffect(() => {
    if (!inTauri) return
    const button = ref.current
    if (!button) return

    let cancelled = false
    // Last reported bounds, so an observer that fires without a real change
    // (they fire on first observe, and on every scroll-driven reflow) does not
    // spend an IPC round-trip repeating itself.
    let last = ''

    const report = () => {
      if (cancelled) return
      const r = button.getBoundingClientRect()
      const key = `${r.x},${r.y},${r.width},${r.height}`
      if (key === last) return
      last = key
      void commands
        .setMaximizeButtonRect({
          x: r.x,
          y: r.y,
          width: r.width,
          height: r.height,
        })
        .catch(() => {})
    }

    report()
    const observer = new ResizeObserver(report)
    observer.observe(button)
    observer.observe(document.documentElement)

    const win = getCurrentWindow()
    const unlisteners = [
      win.listen<boolean>('snap-layouts://hover', (e) => {
        if (!cancelled) setHovered(e.payload)
      }),
      win.listen('snap-layouts://click', () => {
        if (!cancelled) activate.current()
      }),
    ]

    return () => {
      cancelled = true
      observer.disconnect()
      // Give up the claim on the way out. Leaving a stale rectangle behind would
      // keep a strip of the window answering HTMAXBUTTON after the button that
      // justified it is gone, swallowing clicks meant for whatever replaced it.
      void commands.setMaximizeButtonRect(null).catch(() => {})
      for (const un of unlisteners) un.then((u) => u()).catch(() => {})
    }
  }, [])

  return { ref, hovered }
}
