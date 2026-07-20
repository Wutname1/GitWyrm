import { type ReactNode, useState } from 'react'
import { Download, FileText, MinusCircle, PlusCircle, RotateCcw, Trash2 } from 'lucide-react'
import { PendingIndicator } from '@/components/ui/pending-indicator'
import type { FileChange } from '@/lib/bindings'
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
import { useGitMutations } from '@/hooks/useGitMutations'
import { useActiveRepo } from '@/stores/workspaceStore'

interface FileChangeMenuProps {
  file: FileChange
  staged: boolean
  onOpen: () => void
  children: ReactNode
}

export function FileChangeMenu({ file, staged, onOpen, children }: FileChangeMenuProps) {
  const repo = useActiveRepo()
  const m = useGitMutations(repo?.id ?? null)
  const [confirmDiscard, setConfirmDiscard] = useState(false)
  const name = file.path.split('/').pop() ?? file.path
  const stagePending = m.stageFile.isPending || m.unstageFile.isPending
  const sub = file.submodule
  const uninitialized = sub != null && !sub.initialized

  return (
    <>
      <ContextMenu>
        <ContextMenuTrigger asChild>{children}</ContextMenuTrigger>
        <ContextMenuContent className="w-56">
          <ContextMenuLabel className="overflow-hidden text-ellipsis whitespace-nowrap font-mono text-[11px] text-sub">
            {name}
          </ContextMenuLabel>
          <ContextMenuSeparator />
          <ContextMenuItem onSelect={onOpen}>
            <FileText />
            {file.conflicted ? 'Resolve conflicts' : sub ? 'See what moved' : 'View changes'}
          </ContextMenuItem>
          {staged ? (
            <PendingMenuItem
              icon={<MinusCircle />}
              label="Unstage this file"
              pendingLabel="Unstaging file…"
              pending={m.unstageFile.isPending}
              disabled={stagePending}
              onRun={() => m.unstageFile.mutate(file.path)}
            />
          ) : (
            <PendingMenuItem
              icon={<PlusCircle />}
              label="Stage this file"
              pendingLabel="Staging file…"
              pending={m.stageFile.isPending}
              disabled={stagePending}
              onRun={() => m.stageFile.mutate(file.path)}
            />
          )}
          {sub ? (
            <>
              <ContextMenuSeparator />
              {uninitialized ? (
                <PendingMenuItem
                  icon={<Download />}
                  label="Download submodule"
                  pendingLabel="Downloading…"
                  pending={m.updateSubmodule.isPending}
                  disabled={m.updateSubmodule.isPending}
                  onRun={() => m.updateSubmodule.mutate({ path: file.path, init: true })}
                />
              ) : (
                <ContextMenuItem variant="destructive" onSelect={() => setConfirmDiscard(true)}>
                  <RotateCcw />
                  Reset to recorded commit
                </ContextMenuItem>
              )}
            </>
          ) : (
            !file.conflicted && (
              <>
                <ContextMenuSeparator />
                <ContextMenuItem variant="destructive" onSelect={() => setConfirmDiscard(true)}>
                  <Trash2 />
                  Discard changes
                </ContextMenuItem>
              </>
            )
          )}
        </ContextMenuContent>
      </ContextMenu>

      <ConfirmDialog
        open={confirmDiscard}
        onOpenChange={setConfirmDiscard}
        destructive
        title={sub ? `Reset ${name} to the recorded commit?` : `Discard changes in ${name}?`}
        description={
          sub ? (
            <>
              This moves the submodule{' '}
              <span className="font-mono text-foreground">{file.path}</span> back to the commit this
              project records for it, dropping any different commit it currently points to. This
              can't be undone.
            </>
          ) : (
            <>
              This throws away your edits to{' '}
              <span className="font-mono text-foreground">{file.path}</span> and puts the file back
              to the last commit. This can't be undone.
            </>
          )
        }
        confirmLabel={sub ? 'Reset submodule' : 'Discard changes'}
        onConfirm={() =>
          sub
            ? m.updateSubmodule.mutate({ path: file.path, init: false })
            : m.discardFile.mutate(file.path)
        }
      />
    </>
  )
}
