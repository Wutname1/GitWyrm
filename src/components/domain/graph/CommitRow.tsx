import { memo } from 'react'
import { cn } from '@/lib/utils'
import type { CommitEntry } from '@/lib/bindings'
import { authorColor, formatCommitTime } from '@/lib/gitDisplay'
import { effectiveHiddenColumns, gridTemplate, visibleColumns, type ColumnId } from '@/lib/graphColumns'
import { useWorkspaceStore } from '@/stores/workspaceStore'
import { Avatar } from './Avatar'
import { RefBadge } from './RefBadge'
import { RefStack } from './RefStack'
import { CommitContextMenu } from './CommitContextMenu'
import { ChangeSizeIndicator } from './ChangeSizeIndicator'

interface CommitRowProps {
  commit: CommitEntry
  selected: boolean
  onSelect: () => void
  rowHeight: number
  style?: React.CSSProperties
}

export const CommitRow = memo(function CommitRow({ commit, selected, onSelect, rowHeight, style }: CommitRowProps) {
  const order = useWorkspaceStore((s) => s.columnOrder)
  const hidden = useWorkspaceStore((s) => s.hiddenColumns)
  const widths = useWorkspaceStore((s) => s.columnWidths)
  const changeSizeDisplay = useWorkspaceStore((s) => s.changeSizeDisplay)
  const showChangeIndicator = useWorkspaceStore((s) => s.showChangeIndicator)
  const showLineCounts = useWorkspaceStore((s) => s.showChangeLineCounts)
  const effectiveHidden = effectiveHiddenColumns(hidden, showChangeIndicator, changeSizeDisplay)
  const color = authorColor(commit.author_email || commit.author_name)

  const cell: Record<ColumnId, React.ReactNode> = {
    refs: (
      <div className="gw-refs-cell flex items-center gap-1 overflow-hidden pr-1.5">
        {commit.refs.length > 1 ? (
          <RefStack refs={commit.refs} />
        ) : (
          commit.refs.map((r) => <RefBadge key={`${r.type}:${r.name}`} refTag={r} />)
        )}
      </div>
    ),
    graph: <div />,
    message: (
      <div
        data-dim-on-drag
        className={cn(
          'min-w-0 overflow-hidden pr-2.5 text-foreground',
          showChangeIndicator && changeSizeDisplay === 'row' && 'flex h-full flex-col justify-center',
        )}
      >
        <div className="overflow-hidden text-ellipsis whitespace-nowrap">{commit.summary}</div>
        {showChangeIndicator && changeSizeDisplay === 'row' && (
          <ChangeSizeIndicator
            filesChanged={commit.files_changed}
            additions={commit.additions}
            deletions={commit.deletions}
            showLineCounts={showLineCounts}
            mode="row"
          />
        )}
      </div>
    ),
    author: (
      <div data-dim-on-drag className="flex items-center gap-[7px] overflow-hidden">
        <Avatar initials={commit.author_initials} color={color} email={commit.author_email} />
        <span className="overflow-hidden text-ellipsis whitespace-nowrap text-2xs text-sub">
          {commit.author_name}
        </span>
      </div>
    ),
    changes: (
      <ChangeSizeIndicator
        filesChanged={commit.files_changed}
        additions={commit.additions}
        deletions={commit.deletions}
        showLineCounts={showLineCounts}
        mode="column"
      />
    ),
    date: <div data-dim-on-drag className="font-mono text-2xs text-sub">{formatCommitTime(commit.time)}</div>,
    sha: <div data-dim-on-drag className="font-mono text-2xs text-muted-foreground">{commit.short_sha}</div>,
  }

  return (
    <CommitContextMenu commit={commit} onViewDetails={onSelect}>
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
    </CommitContextMenu>
  )
})
