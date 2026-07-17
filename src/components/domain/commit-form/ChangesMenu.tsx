import { type ReactNode, useState } from 'react'
import { Archive, MinusCircle, PlusCircle, Trash2 } from 'lucide-react'
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuLabel,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from '@/components/ui/context-menu'
import { ConfirmDialog } from '@/components/modals/ConfirmDialog'
import { PendingIndicator } from '@/components/ui/pending-indicator'
import { useStatus } from '@/hooks/useGitQueries'
import { useGitMutations } from '@/hooks/useGitMutations'
import { useActiveRepo } from '@/stores/workspaceStore'

interface ChangesMenuProps {
  children: ReactNode
  /** Radix trigger mode: right-click a row, or as a button dropdown target. */
  asChild?: boolean
}

/** Right-click menu for the whole set of uncommitted changes. */
export function ChangesMenu({ children, asChild = true }: ChangesMenuProps) {
  const repo = useActiveRepo()
  const status = useStatus(repo?.id ?? null)
  const m = useGitMutations(repo?.id ?? null)
  const [confirmDiscard, setConfirmDiscard] = useState(false)

  const staged = status.data?.staged.length ?? 0
  const unstaged = status.data?.unstaged.length ?? 0
  const total = staged + unstaged
  const hasChanges = total > 0
  const operationPending =
    m.stageAll.isPending || m.unstageAll.isPending || m.stashSave.isPending

  return (
    <>
      <ContextMenu>
        <ContextMenuTrigger asChild={asChild}>{children}</ContextMenuTrigger>
        <ContextMenuContent className="w-52">
          <ContextMenuLabel className="text-[11px] text-sub">
            {hasChanges ? `${total} changed file${total === 1 ? '' : 's'}` : 'No changes'}
          </ContextMenuLabel>
          <ContextMenuSeparator />
          <ContextMenuItem
            disabled={unstaged === 0 || operationPending}
            onSelect={(e) => {
              e.preventDefault()
              m.stageAll.mutate()
            }}
          >
            {m.stageAll.isPending ? <PendingIndicator /> : <PlusCircle />}
            {m.stageAll.isPending ? 'Staging all changes…' : 'Stage all changes'}
          </ContextMenuItem>
          <ContextMenuItem
            disabled={staged === 0 || operationPending}
            onSelect={(e) => {
              e.preventDefault()
              m.unstageAll.mutate()
            }}
          >
            {m.unstageAll.isPending ? <PendingIndicator /> : <MinusCircle />}
            {m.unstageAll.isPending ? 'Unstaging all…' : 'Unstage all'}
          </ContextMenuItem>
          <ContextMenuItem
            disabled={!hasChanges || operationPending}
            onSelect={(e) => {
              e.preventDefault()
              m.stashSave.mutate(undefined)
            }}
          >
            {m.stashSave.isPending ? <PendingIndicator /> : <Archive />}
            {m.stashSave.isPending ? 'Stashing changes…' : 'Stash all changes'}
          </ContextMenuItem>
          <ContextMenuSeparator />
          <ContextMenuItem
            variant="destructive"
            disabled={!hasChanges}
            onSelect={() => setConfirmDiscard(true)}
          >
            <Trash2 />
            Discard all changes
          </ContextMenuItem>
        </ContextMenuContent>
      </ContextMenu>

      <ConfirmDialog
        open={confirmDiscard}
        onOpenChange={setConfirmDiscard}
        destructive
        title="Discard all changes?"
        description={
          <>
            This throws away every uncommitted change across{' '}
            <span className="text-foreground">{total}</span> file{total === 1 ? '' : 's'} and puts
            your project back to the last commit. This can't be undone. Consider stashing instead.
          </>
        }
        confirmLabel="Discard everything"
        confirmPhrase="discard"
        onConfirm={() => m.discardAll.mutate()}
      />
    </>
  )
}
