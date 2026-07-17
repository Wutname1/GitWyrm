import { memo } from 'react'
import { cn } from '@/lib/utils'
import type { CommitEntry } from '@/lib/bindings'
import { authorColor, formatCommitTime, GRAPH_ROW_HEIGHT } from '@/lib/gitDisplay'
import { gridTemplate, visibleColumns, type ColumnId } from '@/lib/graphColumns'
import { useWorkspaceStore } from '@/stores/workspaceStore'
import { Avatar } from './Avatar'
import { RefBadge } from './RefBadge'
import { CommitContextMenu } from './CommitContextMenu'

interface CommitRowProps {
  commit: CommitEntry
  selected: boolean
  onSelect: () => void
  style?: React.CSSProperties
}

export const CommitRow = memo(function CommitRow({ commit, selected, onSelect, style }: CommitRowProps) {
  const order = useWorkspaceStore((s) => s.columnOrder)
  const hidden = useWorkspaceStore((s) => s.hiddenColumns)
  const color = authorColor(commit.author_email || commit.author_name)

  const cell: Record<ColumnId, React.ReactNode> = {
    refs: (
      <div className="gw-refs-cell flex items-center gap-1 overflow-hidden pr-1.5">
        {commit.refs.map((r) => (
          <RefBadge key={r.name} refTag={r} />
        ))}
      </div>
    ),
    graph: <div />,
    message: (
      <div data-dim-on-drag className="overflow-hidden text-ellipsis whitespace-nowrap pr-2.5 text-foreground">
        {commit.summary}
      </div>
    ),
    author: (
      <div data-dim-on-drag className="flex items-center gap-[7px] overflow-hidden">
        <Avatar initials={commit.author_initials} color={color} email={commit.author_email} />
        <span className="overflow-hidden text-ellipsis whitespace-nowrap text-[11px] text-sub">
          {commit.author_name}
        </span>
      </div>
    ),
    date: <div data-dim-on-drag className="font-mono text-[11px] text-sub">{formatCommitTime(commit.time)}</div>,
    sha: <div data-dim-on-drag className="font-mono text-[11px] text-muted-foreground">{commit.short_sha}</div>,
  }

  return (
    <CommitContextMenu commit={commit} onViewDetails={onSelect}>
      <div
        onClick={onSelect}
        style={{ height: GRAPH_ROW_HEIGHT, gridTemplateColumns: gridTemplate(order, hidden), ...style }}
        className={cn(
          'grid cursor-pointer items-center pr-1',
          selected && 'bg-soft shadow-[inset_2px_0_0_var(--gw-accent)]'
        )}
      >
        {visibleColumns(order, hidden).map((id) => (
          <div key={id} className="contents">
            {cell[id]}
          </div>
        ))}
      </div>
    </CommitContextMenu>
  )
})
