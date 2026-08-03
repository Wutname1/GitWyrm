import { useState } from 'react'
import { Folder, Plus, Star, Trash2 } from 'lucide-react'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { TooltipButton } from '@/components/ui/tooltip'
import { ConfirmDialog } from '@/components/modals/ConfirmDialog'
import { cn } from '@/lib/utils'
import { primaryFirst, useWorkspaceStore } from '@/stores/workspaceStore'

/**
 * The watched-folder list: add, star a primary, and remove.
 *
 * Shared by the General settings screen and the repository picker's folder
 * manager so both stay in step.
 */
export function CodeFoldersSetting() {
  const codeFolders = useWorkspaceStore((s) => s.codeFolders)
  const addCodeFolder = useWorkspaceStore((s) => s.addCodeFolder)
  const removeCodeFolder = useWorkspaceStore((s) => s.removeCodeFolder)
  const setPrimaryCodeFolder = useWorkspaceStore((s) => s.setPrimaryCodeFolder)
  const [pendingRemoval, setPendingRemoval] = useState<string | null>(null)

  const ordered = primaryFirst(codeFolders)

  const browse = async () => {
    const { open } = await import('@tauri-apps/plugin-dialog')
    const directory = await open({
      directory: true,
      multiple: true,
      title: 'Choose a folder that holds your repositories',
    })
    const paths = Array.isArray(directory) ? directory : typeof directory === 'string' ? [directory] : []
    if (paths.length === 0) return

    const before = useWorkspaceStore.getState().codeFolders.length
    for (const path of paths) addCodeFolder(path)
    const added = useWorkspaceStore.getState().codeFolders.length - before

    if (added === 0) toast('Already watching that folder')
    else toast.success(added === 1 ? 'Folder added' : `${added} folders added`)
  }

  return (
    <div className="grid gap-2">
      {ordered.length > 0 && (
        <div className="grid gap-1">
          {ordered.map((folder) => (
            <div
              key={folder.path}
              className="group/folder flex min-h-9 items-center gap-2 rounded-md border border-border bg-background px-2 py-1"
            >
              <TooltipButton
                tooltip={folder.primary ? 'This is your main folder' : 'Make this your main folder'}
                aria-pressed={folder.primary}
                onClick={() => {
                  if (folder.primary) return
                  setPrimaryCodeFolder(folder.path)
                  toast.success(`${folder.path} is now your main folder`)
                }}
                className={cn(
                  'grid size-6 flex-none place-items-center rounded hover:bg-panel3',
                  folder.primary ? 'text-accent-text' : 'text-muted-foreground',
                )}
              >
                <Star size={12} fill={folder.primary ? 'currentColor' : 'none'} />
              </TooltipButton>
              <span className="min-w-0 flex-1">
                <span className="block truncate font-mono text-xs text-foreground">{folder.path}</span>
                {folder.primary && (
                  <span className="block text-2xs text-muted-foreground">
                    New projects and copies are saved here
                  </span>
                )}
              </span>
              <TooltipButton
                tooltip={`Stop watching ${folder.path}`}
                onClick={() => setPendingRemoval(folder.path)}
                className="grid size-6 flex-none place-items-center rounded text-muted-foreground opacity-0 hover:bg-panel3 hover:text-removed group-hover/folder:opacity-100 focus:opacity-100"
              >
                <Trash2 size={12} />
              </TooltipButton>
            </div>
          ))}
        </div>
      )}

      {ordered.length === 0 && (
        <p className="rounded-md border border-dashed border-border px-3 py-3 text-xs text-muted-foreground">
          No folders yet. Add the folder your projects live in and GitWyrm will find them for you.
        </p>
      )}

      <div className="flex gap-2">
        <Button variant="secondary" size="sm" className="h-8 gap-1.5 text-xs" onClick={browse}>
          <Plus size={13} />
          Add folder
        </Button>
        {ordered.length > 0 && (
          <span className="flex items-center gap-1.5 text-2xs text-muted-foreground">
            <Folder size={11} />
            {ordered.length === 1 ? '1 folder watched' : `${ordered.length} folders watched`}
          </span>
        )}
      </div>

      <ConfirmDialog
        open={pendingRemoval != null}
        onOpenChange={(open) => !open && setPendingRemoval(null)}
        destructive
        title="Stop watching this folder?"
        description={
          <>
            <span className="font-mono text-foreground">{pendingRemoval}</span> will no longer be
            searched for projects. Nothing on your computer is deleted, and you can add it back at
            any time.
          </>
        }
        confirmLabel="Stop watching"
        onConfirm={() => {
          if (!pendingRemoval) return
          removeCodeFolder(pendingRemoval)
          toast.success('Folder removed')
          setPendingRemoval(null)
        }}
      />
    </div>
  )
}
