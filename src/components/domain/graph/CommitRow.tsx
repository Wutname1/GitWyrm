import { memo } from 'react'
import { cn } from '@/lib/utils'
import type { CommitEntry } from '@/lib/bindings'
import { authorColor, formatCommitTime, GRAPH_ROW_HEIGHT } from '@/lib/gitDisplay'
import { Avatar } from './Avatar'
import { RefBadge } from './RefBadge'
import { CommitContextMenu } from './CommitContextMenu'

export const GRAPH_GRID = 'grid-cols-[150px_96px_minmax(190px,1fr)_150px_110px_72px]'

interface CommitRowProps {
  commit: CommitEntry
  selected: boolean
  onSelect: () => void
  style?: React.CSSProperties
}

export const CommitRow = memo(function CommitRow({ commit, selected, onSelect, style }: CommitRowProps) {
  const color = authorColor(commit.author_email || commit.author_name)
  return (
    <CommitContextMenu commit={commit} onViewDetails={onSelect}>
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
          {commit.refs.map((r) => (
            <RefBadge key={r.name} refTag={r} />
          ))}
        </div>
        <div />
        <div className="overflow-hidden text-ellipsis whitespace-nowrap pr-2.5 text-foreground">
          {commit.summary}
        </div>
        <div className="flex items-center gap-[7px] overflow-hidden">
          <Avatar initials={commit.author_initials} color={color} email={commit.author_email} />
          <span className="overflow-hidden text-ellipsis whitespace-nowrap text-[11px] text-sub">
            {commit.author_name}
          </span>
        </div>
        <div className="font-mono text-[11px] text-sub">{formatCommitTime(commit.time)}</div>
        <div className="font-mono text-[11px] text-muted-foreground">{commit.short_sha}</div>
      </div>
    </CommitContextMenu>
  )
})
