import type { ReactNode } from 'react'
import { X } from 'lucide-react'
import { TooltipButton } from '@/components/ui/tooltip'
import { useUiStore } from '@/stores/uiStore'
import { FileViewTabs, type FileViewMode } from './FileViewTabs'

interface FileViewHeaderProps {
  path: string
  /** Which file view is showing, so the others can be offered. */
  mode: FileViewMode
  /** Commit blame is pinned to, shown as a badge when set. */
  pinnedLabel?: string | null
  children?: ReactNode
}

/**
 * Shared top bar for the file history and blame views: the file's name, a
 * switch between the three file views, and a way back to the graph.
 */
export function FileViewHeader({ path, mode, pinnedLabel, children }: FileViewHeaderProps) {
  const showGraph = useUiStore((s) => s.showGraph)

  return (
    <div className="flex h-10 flex-none items-center gap-2.5 border-b border-border bg-panel px-3.5">
      <span className="overflow-hidden text-ellipsis whitespace-nowrap font-mono text-xs text-foreground">
        {path}
      </span>
      {pinnedLabel && (
        <span className="flex-none rounded-full border border-border bg-panel3 px-[7px] py-px font-mono text-2xs font-semibold text-sub">
          {pinnedLabel}
        </span>
      )}

      <div className="flex-1" />

      <FileViewTabs path={path} mode={mode} />

      {children}

      <TooltipButton
        onClick={showGraph}
        tooltip="Back to history graph"
        className="flex size-6 flex-none items-center justify-center rounded-[5px] border border-border bg-panel2 text-xs text-sub hover:border-muted-foreground hover:bg-panel3"
      >
        <X size={12} />
      </TooltipButton>
    </div>
  )
}
