import { useEffect, useState } from 'react'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { Copy, Minus, Square, X } from 'lucide-react'
import { TooltipButton } from '@/components/ui/tooltip'
import { describeError, log } from '@/lib/log'

const inTauri = '__TAURI_INTERNALS__' in window

/**
 * How long to let a normal close finish before forcing it. Long enough for the
 * settings flush on `closeRequested` to complete on a slow disk, short enough
 * that a user who clicked X is not left looking at a window that ignored them.
 */
const CLOSE_GRACE_MS = 1500

/**
 * Close the window, and actually mean it.
 *
 * `close()` raises `closeRequested`, so anything listening on that event -- or
 * any failure inside it -- can leave the window open with nothing to show the
 * user why. This previously swallowed its rejection entirely, which read as a
 * dead button and left no trace in gitwyrm.log.
 *
 * So: log what went wrong, then fall back to `destroy()`, which skips the
 * close-requested path and tears the window down unconditionally. Losing the
 * settings flush that rides on `closeRequested` is a far better outcome than a
 * window the user cannot close.
 */
async function closeWindow() {
  const win = getCurrentWindow()
  try {
    await win.close()
  } catch (e) {
    log.error(`window close failed, forcing destroy: ${describeError(e)}`)
    await force(win)
    return
  }
  // `close()` resolving only means the request was delivered, not honoured: a
  // listener that blocks the close leaves the window up with no rejection to
  // catch. If we are still here after a moment, the request did not take.
  window.setTimeout(() => {
    log.error('window still open after close(); forcing destroy')
    void force(win)
  }, CLOSE_GRACE_MS)
}

async function force(win: ReturnType<typeof getCurrentWindow>) {
  try {
    await win.destroy()
  } catch (e) {
    log.error(`window destroy also failed: ${describeError(e)}`)
  }
}

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
      <TooltipButton
        onClick={() => getCurrentWindow().minimize().catch(() => {})}
        tooltip="Minimize"
        className={btn}
      >
        <Minus size={14} />
      </TooltipButton>
      <TooltipButton
        onClick={() => getCurrentWindow().toggleMaximize().catch(() => {})}
        tooltip={isMax ? 'Restore' : 'Maximize'}
        className={btn}
      >
        {isMax ? <Copy size={12} className="rotate-90" /> : <Square size={12} />}
      </TooltipButton>
      <TooltipButton
        onClick={closeWindow}
        tooltip="Close"
        className="inline-flex h-full w-11 flex-none items-center justify-center text-sub transition-colors hover:bg-red-600 hover:text-white mr-2 rounded-tr"
      >
        <X size={14} />
      </TooltipButton>
    </div>
  )
}
