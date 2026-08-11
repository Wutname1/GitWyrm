import { useState } from 'react'
import { FolderOpen, History, Trash2, Undo2, UserSearch } from 'lucide-react'
import { ContextMenuItem, ContextMenuSeparator } from '@/components/ui/context-menu'
import { PendingMenuItem } from '@/components/ui/pending-menu-item'
import { ConfirmDialog } from '@/components/modals/ConfirmDialog'
import { useGitMutations } from '@/hooks/useGitMutations'
import { useNeverCommitted } from '@/hooks/useGitQueries'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo, useWorkspaceStore } from '@/stores/workspaceStore'
import { EditorGlyph, editorLabel } from '@/lib/editors'

/**
 * Confirm state for the two file actions that ask before acting.
 *
 * It lives outside the menu on purpose. Choosing an item closes the context
 * menu, which unmounts everything rendered inside ContextMenuContent -- a
 * dialog rendered there would open and disappear in the same frame. The owner
 * holds this state and renders {@link FileActionDialogs} as a sibling of the
 * menu, so the dialog outlives the menu that opened it.
 */
export interface FileActionConfirm {
  open: null | 'delete' | 'restore'
  ask: (which: 'delete' | 'restore') => void
  close: () => void
}

export function useFileActionConfirm(): FileActionConfirm {
  const [open, setOpen] = useState<null | 'delete' | 'restore'>(null)
  return {
    open,
    ask: (which) => setOpen(which),
    close: () => setOpen(null),
  }
}

interface FileActionMenuItemsProps {
  path: string
  /**
   * Commit the file is being viewed at, when the menu is opened from a past
   * commit's file list. Blame opens pinned to that commit; the actions that
   * touch the working copy are hidden, because the file on disk today may have
   * nothing to do with the revision on screen.
   */
  sha?: string | null
  confirm: FileActionConfirm
}

/**
 * The file actions shared by every changed-file row: open it elsewhere, remove
 * or restore it, and the two history views. Rendered inside an existing
 * ContextMenuContent, after the caller's own stage/discard entries.
 *
 * The caller must also render {@link FileActionDialogs} outside its
 * `<ContextMenu>`, passing the same `confirm`.
 */
export function FileActionMenuItems({ path, sha = null, confirm }: FileActionMenuItemsProps) {
  const repo = useActiveRepo()
  const m = useGitMutations(repo?.id ?? null)
  const defaultEditor = useWorkspaceStore((s) => s.defaultEditor)
  const openFileHistory = useUiStore((s) => s.openFileHistory)
  const openBlame = useUiStore((s) => s.openBlame)
  const workingCopy = sha == null
  // A file with no commits behind it has no history to show and nothing to be
  // restored to, so both are left out rather than offered and then failing.
  const neverCommitted = useNeverCommitted(repo?.id ?? null, path, sha)

  return (
    <>
      <ContextMenuSeparator />
      <PendingMenuItem
        icon={<EditorGlyph kind={defaultEditor} size={14} className="flex-none" />}
        label={`Open in ${editorLabel(defaultEditor)}`}
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

      {!neverCommitted && (
        <>
          <ContextMenuSeparator />
          <ContextMenuItem onSelect={() => openFileHistory(path)}>
            <History />
            File history
          </ContextMenuItem>
          <ContextMenuItem onSelect={() => openBlame(path, sha)}>
            <UserSearch />
            Blame
          </ContextMenuItem>
        </>
      )}

      {workingCopy && (
        <>
          <ContextMenuSeparator />
          {!neverCommitted && (
            <ContextMenuItem onSelect={() => confirm.ask('restore')}>
              <Undo2 />
              Restore file
            </ContextMenuItem>
          )}
          <ContextMenuItem variant="destructive" onSelect={() => confirm.ask('delete')}>
            <Trash2 />
            Delete file
          </ContextMenuItem>
        </>
      )}
    </>
  )
}

/**
 * The confirmations behind Delete file and Restore file. Render this as a
 * sibling of the `<ContextMenu>`, never inside it -- see
 * {@link FileActionConfirm}.
 */
export function FileActionDialogs({ path, confirm }: { path: string; confirm: FileActionConfirm }) {
  const repo = useActiveRepo()
  const m = useGitMutations(repo?.id ?? null)
  const name = path.split('/').pop() ?? path

  return (
    <>
      <ConfirmDialog
        open={confirm.open === 'delete'}
        onOpenChange={(next) => !next && confirm.close()}
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
        open={confirm.open === 'restore'}
        onOpenChange={(next) => !next && confirm.close()}
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
