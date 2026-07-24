import { useEffect, useState } from 'react'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { Copy, Minus, Square, X } from 'lucide-react'
import logoUrl from '@/assets/logo.png'

const inTauri = '__TAURI_INTERNALS__' in window

/**
 * The app bar shown on the crash screen. It deliberately duplicates the logo and
 * window buttons instead of reusing TabBar/WindowControls: the real ones depend
 * on the stores, the tooltip provider and the tab machinery, any of which may be
 * the thing that just crashed. Without a window to close or drag, a crash leaves
 * the user stuck with no way out but the task manager.
 */
export function CrashTitleBar() {
  const [isMax, setIsMax] = useState(false)

  useEffect(() => {
    if (!inTauri) return
    const win = getCurrentWindow()
    win.isMaximized().then(setIsMax).catch(() => {})
    const un = win.onResized(() => {
      win.isMaximized().then(setIsMax).catch(() => {})
    })
    return () => {
      un.then((u) => u()).catch(() => {})
    }
  }, [])

  const btn =
    'inline-flex h-full w-11 flex-none items-center justify-center text-sub transition-colors hover:bg-panel3 hover:text-foreground'

  const toggleMaximize = () => {
    if (!inTauri) return
    getCurrentWindow().toggleMaximize().catch(() => {})
  }

  return (
    <div
      data-tauri-drag-region
      onDoubleClick={(e) => {
        if ((e.target as HTMLElement).hasAttribute('data-tauri-drag-region')) toggleMaximize()
      }}
      className="flex h-9 flex-none items-stretch border-b border-border bg-background pl-2.5"
    >
      <div data-tauri-drag-region className="flex items-center gap-[7px]">
        <img src={logoUrl} alt="" draggable={false} data-tauri-drag-region className="size-[18px] flex-none" />
        <span
          data-tauri-drag-region
          className="text-[0.84375rem] leading-none"
          style={{ fontFamily: 'var(--font-wordmark)', fontWeight: 600, letterSpacing: '-0.035em' }}
        >
          <span data-tauri-drag-region style={{ color: 'var(--gw-text)' }}>
            Git
          </span>
          <span data-tauri-drag-region style={{ color: 'var(--gw-accent)' }}>
            Wyrm
          </span>
        </span>
      </div>
      <div data-tauri-drag-region className="min-w-0 flex-1" />
      {inTauri && (
        <div className="titlebar-no-drag flex h-full items-stretch">
          <button
            type="button"
            aria-label="Minimize"
            onClick={() => getCurrentWindow().minimize().catch(() => {})}
            className={btn}
          >
            <Minus size={14} />
          </button>
          <button
            type="button"
            aria-label={isMax ? 'Restore' : 'Maximize'}
            onClick={toggleMaximize}
            className={btn}
          >
            {isMax ? <Copy size={12} className="rotate-90" /> : <Square size={12} />}
          </button>
          <button
            type="button"
            aria-label="Close"
            onClick={() => getCurrentWindow().close().catch(() => {})}
            className="inline-flex h-full w-11 flex-none items-center justify-center text-sub transition-colors hover:bg-red-600 hover:text-white"
          >
            <X size={14} />
          </button>
        </div>
      )}
    </div>
  )
}
