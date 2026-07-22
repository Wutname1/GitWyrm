import { useState } from 'react'
import { Code2, FolderOpen, History, Trash2, Undo2, UserSearch } from 'lucide-react'
import { ContextMenuItem, ContextMenuSeparator } from '@/components/ui/context-menu'
import { PendingMenuItem } from '@/components/ui/pending-menu-item'
import { ConfirmDialog } from '@/components/modals/ConfirmDialog'
import { useGitMutations } from '@/hooks/useGitMutations'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo } from '@/stores/workspaceStore'

interface FileActionMenuItemsProps {
  path: string
  /**
   * Commit the file is being viewed at, when the menu is opened from a past
   * commit's file list. Blame opens pinned to that commit; the actions that
   * touch the working copy are hidden, because the file on disk today may have
   * nothing to do with the revision on screen.
   */
  sha?: string | null
}

/**
 * The file actions shared by every changed-file row: open it elsewhere, remove
 * or restore it, and the two history views. Rendered inside an existing
 * ContextMenuContent, after the caller's own stage/discard entries.
 */
export function FileActionMenuItems({ path, sha = null }: FileActionMenuItemsProps) {
  const repo = useActiveRepo()
  const m = useGitMutations(repo?.id ?? null)
  const openFileHistory = useUiStore((s) => s.openFileHistory)
  const openBlame = useUiStore((s) => s.openBlame)
  const [confirmDelete, setConfirmDelete] = useState(false)
  const [confirmRestore, setConfirmRestore] = useState(false)
  const name = path.split('/').pop() ?? path
  const workingCopy = sha == null

  return (
    <>
      <ContextMenuSeparator />
      <PendingMenuItem
        icon={<Code2 />}
        label="Open in VS Code"
        pendingLabel="Opening…"
        pending={m.openFileInEditor.isPending && m.openFileInEditor.variables === path}
        onRun={() => m.openFileInEditor.mutate(path)}
      />
      <PendingMenuItem
        icon={<FolderOpen />}
        label="Show in folder"
        pendingLabel="Opening…"
        pending={m.revealFile.isPending && m.revealFile.variables === path}
        onRun={() => m.revealFile.mutate(path)}
      />

      <ContextMenuSeparator />
      <ContextMenuItem onSelect={() => openFileHistory(path)}>
        <History />
        File history
      </ContextMenuItem>
      <ContextMenuItem onSelect={() => openBlame(path, sha)}>
        <UserSearch />
        Blame
      </ContextMenuItem>

      {workingCopy && (
        <>
          <ContextMenuSeparator />
          <ContextMenuItem onSelect={() => setConfirmRestore(true)}>
            <Undo2 />
            Restore file
          </ContextMenuItem>
          <ContextMenuItem variant="destructive" onSelect={() => setConfirmDelete(true)}>
            <Trash2 />
            Delete file
          </ContextMenuItem>
        </>
      )}

      <ConfirmDialog
        open={confirmDelete}
        onOpenChange={setConfirmDelete}
        destructive
        title={`Delete ${name}?`}
        description={
          <>
            This moves <span className="font-mono text-foreground">{path}</span> to the Recycle Bin.
            You can put it back from there if you change your mind.
          </>
        }
        confirmLabel="Delete file"
        pending={m.deleteFile.isPending}
        pendingLabel="Deleting…"
        onConfirm={() => m.deleteFile.mutate(path)}
      />

      <ConfirmDialog
        open={confirmRestore}
        onOpenChange={setConfirmRestore}
        destructive
        title={`Restore ${name}?`}
        description={
          <>
            This puts <span className="font-mono text-foreground">{path}</span> back to how it was
            in the last commit, throwing away any changes you have made to it since. This can't be
            undone.
          </>
        }
        confirmLabel="Restore file"
        pending={m.restoreFile.isPending}
        pendingLabel="Restoring…"
        onConfirm={() => m.restoreFile.mutate(path)}
      />
    </>
  )
}
