import { useEffect, useState } from 'react'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { Copy, Minus, Square, X } from 'lucide-react'

const inTauri = '__TAURI_INTERNALS__' in window

export function WindowControls() {
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

  if (!inTauri) return null

  const btn =
    'inline-flex h-full w-11 flex-none items-center justify-center text-sub transition-colors hover:bg-panel3 hover:text-foreground'

  return (
    <div className="titlebar-no-drag -mr-2 flex h-full items-stretch">
      <button
        onClick={() => getCurrentWindow().minimize().catch(() => {})}
        title="Minimize"
        className={btn}
      >
        <Minus size={14} />
      </button>
      <button
        onClick={() => getCurrentWindow().toggleMaximize().catch(() => {})}
        title={isMax ? 'Restore' : 'Maximize'}
        className={btn}
      >
        {isMax ? <Copy size={12} className="rotate-90" /> : <Square size={12} />}
      </button>
      <button
        onClick={() => getCurrentWindow().close().catch(() => {})}
        title="Close"
        className="inline-flex h-full w-11 flex-none items-center justify-center text-sub transition-colors hover:bg-red-600 hover:text-white"
      >
        <X size={14} />
      </button>
    </div>
  )
}
