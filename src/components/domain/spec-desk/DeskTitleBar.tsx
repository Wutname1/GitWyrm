import { useEffect, useState } from 'react'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { ArrowLeftToLine, Pin } from 'lucide-react'
import { cn } from '@/lib/utils'
import { WindowControls } from '@/components/domain/WindowControls'
import { TooltipButton } from '@/components/ui/tooltip'
import { describeError, log } from '@/lib/log'

const inTauri = '__TAURI_INTERNALS__' in window

/**
 * Keep the Desk above other windows. This is the whole point of a popout: the
 * user reads the plan while working in an editor, and a Desk that keeps sinking
 * behind it is no better than a tab.
 */
function KeepOnTop() {
  const [pinned, setPinned] = useState(false)

  if (!inTauri) return null

  const toggle = async () => {
    const next = !pinned
    try {
      await getCurrentWindow().setAlwaysOnTop(next)
      setPinned(next)
    } catch (e) {
      // Surface it: a pin button that silently does nothing is worse than one
      // that says it could not.
      log.error(`keep on top failed: ${describeError(e)}`)
    }
  }

  return (
    <TooltipButton
      onClick={toggle}
      tooltip={pinned ? 'Let other windows cover this one' : 'Keep this window on top'}
      aria-pressed={pinned}
      className={cn(
        'titlebar-no-drag flex h-6 items-center gap-1.5 rounded px-2 text-2xs font-medium transition-colors',
        pinned ? 'bg-soft text-accent-text' : 'text-sub hover:bg-panel3 hover:text-foreground'
      )}
    >
      <Pin size={11} strokeWidth={2.2} className={cn(pinned && 'fill-current')} />
      Keep on top
    </TooltipButton>
  )
}

/** Focus the main window without closing the Desk. */
async function showMainWindow() {
  if (!inTauri) return
  try {
    const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow')
    const main = await WebviewWindow.getByLabel('main')
    if (!main) return
    await main.unminimize()
    await main.show()
    await main.setFocus()
  } catch (e) {
    log.error(`show main window failed: ${describeError(e)}`)
  }
}

/**
 * The Desk's own titlebar. Window decorations are off app-wide, so this draws
 * the drag region and the window buttons, matching the main window's chrome.
 */
export function DeskTitleBar({ repoName }: { repoName: string }) {
  useEffect(() => {
    // Nothing to set up; kept as the seam for future titlebar state.
  }, [])

  return (
    <div
      data-tauri-drag-region
      className="flex h-10 flex-none select-none items-center gap-2.5 border-b border-border bg-panel pl-3 pr-0"
    >
      <span className="size-3.5 flex-none rounded bg-primary" aria-hidden />
      <span className="text-xs font-semibold text-foreground">Spec Desk</span>
      <span className="overflow-hidden text-ellipsis whitespace-nowrap text-2xs text-muted-foreground">
        {repoName}
      </span>

      <div className="titlebar-no-drag ml-3 flex items-center gap-1">
        <KeepOnTop />
        <TooltipButton
          onClick={showMainWindow}
          tooltip="Bring the main GitWyrm window to the front"
          className="flex h-6 items-center gap-1.5 rounded px-2 text-2xs font-medium text-sub transition-colors hover:bg-panel3 hover:text-foreground"
        >
          <ArrowLeftToLine size={11} strokeWidth={2.2} />
          Show main window
        </TooltipButton>
      </div>

      <div className="ml-auto flex h-full items-stretch">
        <WindowControls />
      </div>
    </div>
  )
}
