import type { ReactNode } from 'react'
import { X } from 'lucide-react'
import { cn } from '@/lib/utils'
import { Button } from '@/components/ui/button'
import { TooltipButton } from '@/components/ui/tooltip'
import { useUiStore } from '@/stores/uiStore'

interface FileViewHeaderProps {
  path: string
  /** Which of the two file views is showing, so the other one can be offered. */
  mode: 'history' | 'blame'
  /** Commit blame is pinned to, shown as a badge when set. */
  pinnedLabel?: string | null
  children?: ReactNode
}

/**
 * Shared top bar for the file history and blame views: the file's name, a
 * switch between the two, and a way back to the graph.
 */
export function FileViewHeader({ path, mode, pinnedLabel, children }: FileViewHeaderProps) {
  const showGraph = useUiStore((s) => s.showGraph)
  const openFileHistory = useUiStore((s) => s.openFileHistory)
  const openBlame = useUiStore((s) => s.openBlame)

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

      <div className="flex flex-none items-center rounded border border-border bg-panel2 p-px">
        {(
          [
            ['history', 'History', () => openFileHistory(path)],
            ['blame', 'Blame', () => openBlame(path)],
          ] as const
        ).map(([key, label, go]) => (
          <Button
            key={key}
            size="sm"
            variant="ghost"
            onClick={go}
            aria-pressed={mode === key}
            className={cn(
              'h-auto rounded-[3px] px-2 py-0.5 text-2xs font-semibold',
              mode === key
                ? 'bg-soft text-accent-text hover:bg-soft'
                : 'text-sub hover:bg-panel3 hover:text-foreground'
            )}
          >
            {label}
          </Button>
        ))}
      </div>

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
