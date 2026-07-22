import { useState, type ReactNode } from 'react'
import { ArchiveRestore, Trash2 } from 'lucide-react'
import { cn } from '@/lib/utils'
import type { StashInfo } from '@/lib/bindings'
import { formatCommitTime } from '@/lib/gitDisplay'
import { effectiveHiddenColumns, gridTemplate, visibleColumns, type ColumnId } from '@/lib/graphColumns'
import { useActiveRepo, useWorkspaceStore } from '@/stores/workspaceStore'
import { useGitMutations } from '@/hooks/useGitMutations'
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuLabel,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from '@/components/ui/context-menu'
import { PendingMenuItem } from '@/components/ui/pending-menu-item'
import { ConfirmDialog } from '@/components/modals/ConfirmDialog'
import { ChangeSizeIndicator } from './ChangeSizeIndicator'

/**
 * Right-click menu for a stash, shared by the graph row and the sidebar row.
 * Applying only happens from here (or the sidebar hover action) -- plain
 * clicks never touch the working tree.
 */
export function StashContextMenu({ stash, children }: { stash: StashInfo; children: ReactNode }) {
  const repo = useActiveRepo()
  const m = useGitMutations(repo?.id ?? null)
  const [confirmDelete, setConfirmDelete] = useState(false)
  const busy = m.stashPop.isPending || m.stashApply.isPending || m.stashDrop.isPending

  return (
    <>
      <ContextMenu>
        <ContextMenuTrigger asChild>{children}</ContextMenuTrigger>
        <ContextMenuContent className="w-60">
          <ContextMenuLabel className="text-2xs text-sub">
            Stashed {formatCommitTime(stash.time)}
            {stash.branch ? ` on ${stash.branch}` : ''}
          </ContextMenuLabel>
          <ContextMenuSeparator />
          <PendingMenuItem
            icon={<ArchiveRestore />}
            label="Apply and keep stash"
            pendingLabel="Applying stash…"
            pending={m.stashApply.isPending}
            disabled={busy}
            onRun={() => m.stashApply.mutate(stash.index)}
          />
          <PendingMenuItem
            icon={<ArchiveRestore />}
            label="Apply and remove stash"
            pendingLabel="Applying stash…"
            pending={m.stashPop.isPending}
            disabled={busy}
            onRun={() => m.stashPop.mutate(stash.index)}
          />
          <ContextMenuSeparator />
          <ContextMenuItem variant="destructive" disabled={busy} onSelect={() => setConfirmDelete(true)}>
            <Trash2 />
            Delete stash
          </ContextMenuItem>
        </ContextMenuContent>
      </ContextMenu>

      <ConfirmDialog
        open={confirmDelete}
        onOpenChange={setConfirmDelete}
        destructive
        title="Delete this stash?"
        description={
          <>
            This throws away the stashed changes to{' '}
            <span className="text-foreground">{stash.files_changed}</span> file
            {stash.files_changed === 1 ? '' : 's'} without applying them. This can't be undone.
          </>
        }
        confirmLabel="Delete stash"
        onConfirm={() => m.stashDrop.mutate(stash.index)}
      />
    </>
  )
}

interface StashRowProps {
  stash: StashInfo
  rowHeight: number
  selected: boolean
  onSelect: () => void
  style?: React.CSSProperties
}

/**
 * A stash rendered as a graph row, anchored above the commit it was taken on.
 * Click selects it (the drawer shows the stashed files); applying or deleting
 * happens through the right-click menu so nothing touches the working tree by
 * surprise.
 */
export function StashRow({ stash, rowHeight, selected, onSelect, style }: StashRowProps) {
  const order = useWorkspaceStore((s) => s.columnOrder)
  const hidden = useWorkspaceStore((s) => s.hiddenColumns)
  const widths = useWorkspaceStore((s) => s.columnWidths)
  const changeSizeDisplay = useWorkspaceStore((s) => s.changeSizeDisplay)
  const showChangeIndicator = useWorkspaceStore((s) => s.showChangeIndicator)
  const showLineCounts = useWorkspaceStore((s) => s.showChangeLineCounts)
  const effectiveHidden = effectiveHiddenColumns(hidden, showChangeIndicator, changeSizeDisplay)

  const cell: Record<ColumnId, React.ReactNode> = {
    refs: <div />,
    graph: <div />,
    message: (
      <div
        className={cn(
          'min-w-0 overflow-hidden pr-2.5 text-sub',
          showChangeIndicator && changeSizeDisplay === 'row' && 'flex h-full flex-col justify-center',
        )}
      >
        <div className="overflow-hidden text-ellipsis whitespace-nowrap">{stash.summary}</div>
        {showChangeIndicator && changeSizeDisplay === 'row' && (
          <ChangeSizeIndicator
            filesChanged={stash.files_changed}
            additions={stash.additions}
            deletions={stash.deletions}
            showLineCounts={showLineCounts}
            mode="row"
          />
        )}
      </div>
    ),
    author: <div className="text-2xs italic text-muted-foreground">You</div>,
    changes: (
      <ChangeSizeIndicator
        filesChanged={stash.files_changed}
        additions={stash.additions}
        deletions={stash.deletions}
        showLineCounts={showLineCounts}
        mode="column"
      />
    ),
    date: <div className="font-mono text-2xs text-sub">{formatCommitTime(stash.time)}</div>,
    sha: <div className="font-mono text-2xs text-muted-foreground">{stash.sha.slice(0, 7)}</div>,
  }

  return (
    <StashContextMenu stash={stash}>
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
    </StashContextMenu>
  )
}
