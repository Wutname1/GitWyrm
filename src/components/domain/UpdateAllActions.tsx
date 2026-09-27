import { FolderSync, ListChecks, ListFilter, Loader2, Square } from 'lucide-react'
import type { ReactNode } from 'react'
import { ContextMenuItem } from '@/components/ui/context-menu'
import { DropdownMenuItem } from '@/components/ui/dropdown-menu'
import { Button } from '@/components/ui/button'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { plural } from '@/lib/gitDisplay'
import { cn } from '@/lib/utils'
import { everyProjectRequest, startUpdateAll, stopUpdateAll, useUpdateAllStore } from '@/stores/updateAllStore'
import { useWorkspaceStore } from '@/stores/workspaceStore'

/**
 * "Get the latest for all projects" as a labelled button, for the code folders
 * list on the home screen, where the projects it will touch are on screen.
 */
export function UpdateAllFoldersButton() {
  const progress = useUpdateAllStore((s) => s.progress)
  const openDialog = useUpdateAllStore((s) => s.openDialog)
  const running = progress != null
  return (
    <>
      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            variant="ghost"
            size="sm"
            className={cn('h-7 gap-1.5 text-2xs', running && 'text-accent-text')}
            onClick={() => (running ? openDialog() : void startUpdateAll(everyProjectRequest()))}
          >
            {running ? (
              <Loader2 aria-hidden size={12} className="animate-spin" />
            ) : (
              <FolderSync aria-hidden size={12} />
            )}
            {running
              ? progress.total > 0
                ? `Getting the latest ${progress.done}/${progress.total}`
                : 'Getting the latest…'
              : 'Get the latest for all'}
          </Button>
        </TooltipTrigger>
        <TooltipContent>
          {running
            ? 'Show progress'
            : 'Bring every branch in these projects up to date. Branches with their own new work, or with unsaved changes, are left alone.'}
        </TooltipContent>
      </Tooltip>
      {!running && (
        <Tooltip>
          <TooltipTrigger asChild>
            <Button variant="ghost" size="sm" className="h-7 gap-1.5 text-2xs" onClick={openDialog}>
              <ListFilter aria-hidden size={12} />
              Choose…
            </Button>
          </TooltipTrigger>
          <TooltipContent>Pick which projects to bring up to date</TooltipContent>
        </Tooltip>
      )}
    </>
  )
}

interface UpdateAllEntry {
  key: string
  icon: ReactNode
  label: string
  disabled?: boolean
  run: () => void
}

/**
 * The "get the latest" actions for a menu: start (or, while running, show
 * progress and stop), then the last results. `group` adds a run over just
 * that tab group's projects.
 */
function useUpdateAllEntries(group?: { name: string; paths: string[] }): UpdateAllEntry[] {
  const progress = useUpdateAllStore((s) => s.progress)
  const hasResults = useUpdateAllStore((s) => s.results != null)
  const openResults = useUpdateAllStore((s) => s.openResults)
  const openDialog = useUpdateAllStore((s) => s.openDialog)
  const folderCount = useWorkspaceStore((s) => s.codeFolders.length)
  const openCount = useWorkspaceStore((s) => s.openRepos.length)
  const nothingToUpdate = folderCount === 0 && openCount === 0

  const entries: UpdateAllEntry[] = []
  if (progress) {
    entries.push(
      {
        key: 'progress',
        icon: <Loader2 aria-hidden size={13} className="animate-spin" />,
        label:
          progress.total > 0
            ? `Getting the latest: ${progress.done} of ${plural(progress.total, 'project')}`
            : 'Getting the latest…',
        run: () => openDialog(),
      },
      {
        key: 'stop',
        icon: <Square aria-hidden size={13} />,
        label: progress.stopping ? 'Stopping…' : 'Stop getting the latest',
        disabled: progress.stopping,
        run: () => void stopUpdateAll(),
      },
    )
  } else {
    if (group && group.paths.length > 0) {
      entries.push({
        key: 'group',
        icon: <FolderSync aria-hidden size={13} />,
        label: `Get the latest for ${group.name}`,
        run: () =>
          void startUpdateAll({
            folders: [],
            paths: group.paths,
            allow_set_aside: [],
            sign_in: false,
          }),
      })
    }
    entries.push({
      key: 'all',
      icon: <FolderSync aria-hidden size={13} />,
      label: nothingToUpdate
        ? 'Get the latest for all projects (add a code folder first)'
        : 'Get the latest for all projects',
      disabled: nothingToUpdate,
      run: () => void startUpdateAll(everyProjectRequest()),
    })
    entries.push({
      key: 'choose',
      icon: <ListFilter aria-hidden size={13} />,
      label: 'Get the latest for…',
      disabled: nothingToUpdate,
      run: () => openDialog(),
    })
  }
  if (hasResults) {
    entries.push({
      key: 'results',
      icon: <ListChecks aria-hidden size={13} />,
      label: 'See last results',
      run: () => openResults(),
    })
  }
  return entries
}

/** The actions as dropdown items, for the open-and-recent menu beside the tabs. */
export function UpdateAllDropdownItems() {
  return useUpdateAllEntries().map((entry) => (
    <DropdownMenuItem key={entry.key} className="text-xs" disabled={entry.disabled} onSelect={entry.run}>
      {entry.icon}
      {entry.label}
    </DropdownMenuItem>
  ))
}

/** The actions as right-click items, for tabs and tab groups. */
export function UpdateAllContextItems({ group }: { group?: { name: string; paths: string[] } }) {
  return useUpdateAllEntries(group).map((entry) => (
    <ContextMenuItem key={entry.key} disabled={entry.disabled} onSelect={entry.run}>
      {entry.icon}
      {entry.label}
    </ContextMenuItem>
  ))
}
