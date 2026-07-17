import { type ReactNode, useState } from 'react'
import { FileText, MinusCircle, PlusCircle, Trash2 } from 'lucide-react'
import type { FileChange } from '@/lib/bindings'
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuLabel,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from '@/components/ui/context-menu'
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
            {file.conflicted ? 'Resolve conflicts' : 'View changes'}
          </ContextMenuItem>
          {staged ? (
            <ContextMenuItem onSelect={() => m.unstageFile.mutate(file.path)}>
              <MinusCircle />
              Unstage this file
            </ContextMenuItem>
          ) : (
            <ContextMenuItem onSelect={() => m.stageFile.mutate(file.path)}>
              <PlusCircle />
              Stage this file
            </ContextMenuItem>
          )}
          {!file.conflicted && (
            <>
              <ContextMenuSeparator />
              <ContextMenuItem variant="destructive" onSelect={() => setConfirmDiscard(true)}>
                <Trash2 />
                Discard changes
              </ContextMenuItem>
            </>
          )}
        </ContextMenuContent>
      </ContextMenu>

      <ConfirmDialog
        open={confirmDiscard}
        onOpenChange={setConfirmDiscard}
        destructive
        title={`Discard changes in ${name}?`}
        description={
          <>
            This throws away your edits to{' '}
            <span className="font-mono text-foreground">{file.path}</span> and puts the file back to
            the last commit. This can't be undone.
          </>
        }
        confirmLabel="Discard changes"
        onConfirm={() => m.discardFile.mutate(file.path)}
      />
    </>
  )
}
