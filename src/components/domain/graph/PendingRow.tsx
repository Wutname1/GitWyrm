import { cn } from '@/lib/utils'
import { GRAPH_ROW_HEIGHT } from '@/lib/gitDisplay'
import { GRAPH_GRID } from './CommitRow'

interface PendingRowProps {
  stagedCount: number
  unstagedCount: number
  selected: boolean
  onSelect: () => void
  style?: React.CSSProperties
}

export function PendingRow({ stagedCount, unstagedCount, selected, onSelect, style }: PendingRowProps) {
  const total = stagedCount + unstagedCount
  return (
    <div
      onClick={onSelect}
      style={{ height: GRAPH_ROW_HEIGHT, ...style }}
      className={cn(
        'grid cursor-pointer items-center pr-1',
        GRAPH_GRID,
        selected && 'bg-soft shadow-[inset_2px_0_0_var(--gw-accent)]'
      )}
    >
      <div className="flex items-center gap-1 overflow-hidden pr-1.5">
        <span className="flex items-center gap-1 rounded border border-dashed border-primary/50 bg-soft px-1.5 py-px text-[9.5px] font-semibold uppercase tracking-[.04em] text-primary">
          <span className="relative flex size-1.5 items-center justify-center">
            <span className="absolute inline-flex size-1.5 animate-ping rounded-full bg-primary/60" />
            <span className="relative inline-flex size-1 rounded-full bg-primary" />
          </span>
          WIP
        </span>
      </div>
      <div />
      <div className="overflow-hidden text-ellipsis whitespace-nowrap pr-2.5 text-sub">
        Uncommitted changes
        <span className="ml-2 font-mono text-[10px] text-muted-foreground">
          {stagedCount > 0 && <span className="text-primary">{stagedCount} staged</span>}
          {stagedCount > 0 && unstagedCount > 0 && <span> · </span>}
          {unstagedCount > 0 && <span className="text-modified">{unstagedCount} unstaged</span>}
        </span>
      </div>
      <div className="text-[11px] italic text-muted-foreground">You</div>
      <div className="font-mono text-[11px] text-muted-foreground">now</div>
      <div className="font-mono text-[11px] text-muted-foreground">
        {total} file{total === 1 ? '' : 's'}
      </div>
    </div>
  )
}
