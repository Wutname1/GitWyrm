import { getCurrentWindow } from '@tauri-apps/api/window'
import { Copy, Minus, Square, X } from 'lucide-react'
import { TooltipButton } from '@/components/ui/tooltip'
import { useIsMaximized } from '@/hooks/useIsMaximized'
import { describeError, log } from '@/lib/log'

const inTauri = '__TAURI_INTERNALS__' in window

/**
 * Close the window, reporting a refusal instead of hiding it.
 *
 * A `destroy()` fallback was tried here and removed: it needs
 * `core:window:allow-destroy`, so on any host built before that permission was
 * granted every click raised an unhandled "not allowed by ACL" rejection, which
 * is a worse failure than the one it was meant to cover. Nothing in the app
 * vetoes a close anyway -- the `closeRequested` listener in App.tsx only
 * flushes settings and never calls `preventDefault` -- so `close()` is the whole
 * mechanism, and a failure is worth surfacing rather than working around.
 */
async function closeWindow() {
  try {
    await getCurrentWindow().close()
  } catch (e) {
    // Swallowing this is what made the button look dead with nothing in
    // gitwyrm.log to explain it. Report it instead: the message names the cause
    // (an ACL refusal, a listener that vetoed the close) far better than a
    // guess made here could.
    log.error(`window close failed: ${describeError(e)}`)
  }
}

export function WindowControls() {
  const isMax = useIsMaximized()

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
