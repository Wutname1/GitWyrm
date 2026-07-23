// Dev-only launcher. Renders a small floating button (bottom-left) that opens
// the Theme Lab popout window, and installs the main-window listener that
// applies previews broadcast from that popout. Gated to DEV builds -- it never
// reaches end users.
//
// Mounted once from App. Delete alongside ThemeLab.tsx / themeLab.ts.
import { useEffect, useState } from 'react'
import { WebviewWindow } from '@tauri-apps/api/webviewWindow'
import { listen } from '@tauri-apps/api/event'
import {
  applyThemeOverride,
  clearThemeOverride,
  THEME_PREVIEW_EVENT,
  type ThemePreviewPayload,
} from '@/lib/themeLab'

const LAB_LABEL = 'theme-lab'

function openLab() {
  const existing = WebviewWindow.getByLabel(LAB_LABEL)
  void existing.then((win) => {
    if (win) {
      void win.setFocus()
      return
    }
    const lab = new WebviewWindow(LAB_LABEL, {
      url: 'index.html#theme-lab',
      title: 'Theme Lab',
      width: 380,
      height: 620,
      resizable: true,
      decorations: false,
      // Sit beside the main window rather than centered over it.
      x: 40,
      y: 80,
    })
    lab.once('tauri://error', (e) => {
      console.error('Theme Lab window failed to open:', e)
    })
  })
}

export function ThemeLabLauncher() {
  const [open, setOpen] = useState(false)

  // Main-window listener: apply previews the popout broadcasts. A null payload
  // clears the override, reverting to the committed index.css theme.
  useEffect(() => {
    const unlisten = listen<ThemePreviewPayload>(
      THEME_PREVIEW_EVENT,
      (event) => {
        if (event.payload) {
          applyThemeOverride(event.payload)
          setOpen(true)
        } else {
          clearThemeOverride()
          setOpen(false)
        }
      },
    )
    return () => {
      void unlisten.then((f) => f())
    }
  }, [])

  return (
    <button
      onClick={openLab}
      title="Open Theme Lab (dev)"
      className="titlebar-no-drag fixed bottom-3 left-3 z-[9999] flex items-center gap-1.5 rounded-full border border-border bg-panel/90 px-2.5 py-1 text-2xs text-sub shadow-lg backdrop-blur hover:text-foreground"
    >
      <span
        className="inline-block size-2.5 rounded-full"
        style={{ background: 'var(--gw-accent)' }}
      />
      {open ? 'Theme Lab (previewing)' : 'Theme Lab'}
    </button>
  )
}
