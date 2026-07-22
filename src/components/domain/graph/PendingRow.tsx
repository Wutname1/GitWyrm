import { cn } from '@/lib/utils'
import { effectiveHiddenColumns, gridTemplate, visibleColumns, type ColumnId } from '@/lib/graphColumns'
import { useWorkspaceStore } from '@/stores/workspaceStore'
import { ChangesMenu } from '../commit-form/ChangesMenu'
import { ChangeSizeIndicator } from './ChangeSizeIndicator'

interface PendingRowProps {
  stagedCount: number
  unstagedCount: number
  filesChanged: number
  additions: number
  deletions: number
  rowHeight: number
  selected: boolean
  onSelect: () => void
  style?: React.CSSProperties
}

export function PendingRow({
  stagedCount,
  unstagedCount,
  filesChanged,
  additions,
  deletions,
  rowHeight,
  selected,
  onSelect,
  style,
}: PendingRowProps) {
  const order = useWorkspaceStore((s) => s.columnOrder)
  const hidden = useWorkspaceStore((s) => s.hiddenColumns)
  const widths = useWorkspaceStore((s) => s.columnWidths)
  const changeSizeDisplay = useWorkspaceStore((s) => s.changeSizeDisplay)
  const showChangeIndicator = useWorkspaceStore((s) => s.showChangeIndicator)
  const showLineCounts = useWorkspaceStore((s) => s.showChangeLineCounts)
  const effectiveHidden = effectiveHiddenColumns(hidden, showChangeIndicator, changeSizeDisplay)

  const cell: Record<ColumnId, React.ReactNode> = {
    refs: (
      <div className="flex items-center gap-1 overflow-hidden pr-1.5">
        <span className="flex items-center gap-1 rounded border border-dashed border-primary/50 bg-soft px-1.5 py-px text-2xs font-semibold uppercase tracking-[.04em] text-accent-text">
          <span className="relative flex size-1.5 items-center justify-center">
            <span className="absolute inline-flex size-1.5 animate-ping rounded-full bg-primary/60" />
            <span className="relative inline-flex size-1 rounded-full bg-primary" />
          </span>
          WIP
        </span>
      </div>
    ),
    graph: <div />,
    message: (
      <div className={cn(
        'min-w-0 overflow-hidden pr-2.5 text-sub',
        showChangeIndicator && changeSizeDisplay === 'row' && 'flex h-full flex-col justify-center',
      )}>
        <div className="overflow-hidden text-ellipsis whitespace-nowrap">
          Uncommitted changes
          <span className="ml-2 font-mono text-2xs text-muted-foreground">
            {stagedCount > 0 && <span className="text-accent-text">{stagedCount} staged</span>}
            {stagedCount > 0 && unstagedCount > 0 && <span> · </span>}
            {unstagedCount > 0 && <span className="text-modified">{unstagedCount} unstaged</span>}
          </span>
        </div>
        {showChangeIndicator && changeSizeDisplay === 'row' && (
          <ChangeSizeIndicator
            filesChanged={filesChanged}
            additions={additions}
            deletions={deletions}
            showLineCounts={showLineCounts}
            mode="row"
          />
        )}
      </div>
    ),
    author: <div className="text-2xs italic text-muted-foreground">You</div>,
    changes: (
      <ChangeSizeIndicator
        filesChanged={filesChanged}
        additions={additions}
        deletions={deletions}
        showLineCounts={showLineCounts}
        mode="column"
      />
    ),
    date: <div className="font-mono text-2xs text-muted-foreground">now</div>,
    sha: (
      <div className="font-mono text-2xs text-muted-foreground">
        {filesChanged} file{filesChanged === 1 ? '' : 's'}
      </div>
    ),
  }

  return (
    <ChangesMenu>
      <div
        onClick={onSelect}
        style={{ height: rowHeight, gridTemplateColumns: gridTemplate(order, effectiveHidden, widths), ...style }}
        className={cn(
          'grid cursor-pointer items-center pr-1',
          selected && 'bg-soft shadow-[inset_2px_0_0_var(--gw-accent)]'
        )}
      >
        {visibleColumns(order, effectiveHidden).map((id) => (
          <div key={id} className="contents">
            {cell[id]}
          </div>
        ))}
      </div>
    </ChangesMenu>
  )
}
