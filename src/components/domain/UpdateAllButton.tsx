import { FolderSync, ListChecks, Loader2, Square } from 'lucide-react'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { Button } from '@/components/ui/button'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { showUpdateAllProgressToast } from '@/components/domain/UpdateAllSync'
import { plural } from '@/lib/gitDisplay'
import { cn } from '@/lib/utils'
import { everyProjectRequest, startUpdateAll, stopUpdateAll, useUpdateAllStore } from '@/stores/updateAllStore'
import { useWorkspaceStore } from '@/stores/workspaceStore'

/**
 * The same action as a labelled button, for the code folders list on the home
 * screen, where the projects it will touch are right there on screen.
 */
export function UpdateAllFoldersButton() {
  const progress = useUpdateAllStore((s) => s.progress)
  const running = progress != null
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          variant="ghost"
          size="sm"
          className={cn('h-7 gap-1.5 text-2xs', running && 'text-accent-text')}
          onClick={() => (running ? showUpdateAllProgressToast() : void startUpdateAll(everyProjectRequest()))}
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
  )
}

/**
 * App-bar entry for "get the latest for all projects". A menu rather than a
 * one-click button: the run reaches every server the user has a project on,
 * which is too much to set off by brushing past it.
 */
export function UpdateAllButton() {
  const progress = useUpdateAllStore((s) => s.progress)
  const hasResults = useUpdateAllStore((s) => s.results != null)
  const openResults = useUpdateAllStore((s) => s.openResults)
  const folderCount = useWorkspaceStore((s) => s.codeFolders.length)
  const openCount = useWorkspaceStore((s) => s.openRepos.length)
  const running = progress != null
  const nothingToUpdate = folderCount === 0 && openCount === 0

  const tooltip = running
    ? progress.total > 0
      ? `Getting the latest: ${progress.done} of ${plural(progress.total, 'project')}`
      : 'Getting the latest…'
    : 'Get the latest for all projects'

  return (
    <DropdownMenu>
      <Tooltip>
        <TooltipTrigger asChild>
          <DropdownMenuTrigger
            aria-label="Get the latest for all projects"
            className={cn(
              'flex items-center rounded-[5px] px-2 transition-colors hover:bg-panel3 hover:text-foreground active:bg-panel2 active:text-foreground',
              running ? 'text-accent-text' : 'text-sub',
            )}
          >
            {running ? (
              <Loader2 size={15} strokeWidth={2} className="animate-spin" />
            ) : (
              <FolderSync size={15} strokeWidth={1.9} />
            )}
          </DropdownMenuTrigger>
        </TooltipTrigger>
        <TooltipContent>{tooltip}</TooltipContent>
      </Tooltip>
      <DropdownMenuContent align="end" className="w-72">
        <DropdownMenuLabel className="text-2xs font-normal leading-4 text-muted-foreground">
          Brings every branch in every project up to date with its server. Branches with their own new work, or with
          unsaved changes, are left alone.
        </DropdownMenuLabel>
        <DropdownMenuSeparator />
        {running ? (
          <>
            <DropdownMenuItem className="text-xs" onSelect={() => showUpdateAllProgressToast()}>
              <Loader2 aria-hidden size={13} className="animate-spin" />
              Show progress
            </DropdownMenuItem>
            <DropdownMenuItem
              className="text-xs"
              disabled={progress.stopping}
              onSelect={() => void stopUpdateAll()}
            >
              <Square aria-hidden size={13} />
              {progress.stopping ? 'Stopping…' : 'Stop'}
            </DropdownMenuItem>
          </>
        ) : (
          <DropdownMenuItem
            className="text-xs"
            disabled={nothingToUpdate}
            onSelect={() => void startUpdateAll(everyProjectRequest())}
          >
            <FolderSync aria-hidden size={13} />
            {nothingToUpdate ? 'Add a code folder first' : 'Get the latest for all projects'}
          </DropdownMenuItem>
        )}
        <DropdownMenuItem className="text-xs" disabled={!hasResults} onSelect={() => openResults()}>
          <ListChecks aria-hidden size={13} />
          See last results
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
