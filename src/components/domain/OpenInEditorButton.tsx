import { useQuery } from '@tanstack/react-query'
import { ChevronDown } from 'lucide-react'
import { commands, type SolutionFile } from '@/lib/bindings'
import {
  EditorGlyph,
  EditorWithSolutionGlyph,
  VisualStudioGlyph,
  editorLabel,
} from '@/lib/editors'
import { unwrap } from '@/lib/queryKeys'
import { cn } from '@/lib/utils'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { Tooltip, TooltipButton, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { useGitMutations } from '@/hooks/useGitMutations'
import { useActiveRepo, useWorkspaceStore } from '@/stores/workspaceStore'

const buttonClass =
  'group flex h-[30px] items-center justify-center border border-border bg-panel2 text-sub hover:border-muted-foreground hover:bg-panel3'

/**
 * Toolbar action for handing the repository to a code editor.
 *
 * Clicking always opens the editor chosen in Settings. When Visual Studio is
 * installed and the repository holds `.sln` files, the button grows a dropdown
 * listing each solution, because a C# repo is often more useful opened as a
 * solution than as a folder.
 */
export function OpenInEditorButton({ onNoRepo }: { onNoRepo: () => void }) {
  const repo = useActiveRepo()
  const m = useGitMutations(repo?.id ?? null)
  const defaultEditor = useWorkspaceStore((s) => s.defaultEditor)

  // Availability depends on what is installed and on this repo's solutions, so
  // it is keyed by repo and refetched only when the repo changes.
  const availability = useQuery({
    queryKey: ['editorAvailability', repo?.id ?? null],
    queryFn: async () => unwrap(await commands.getEditorAvailability(repo?.id ?? null)),
    staleTime: 60_000,
  })

  const solutions = availability.data?.solutions ?? []
  const visualStudio = availability.data?.visual_studio ?? null
  const hasSolutions = visualStudio != null && solutions.length > 0

  const label = editorLabel(defaultEditor)
  const openEditor = () => {
    if (!repo) {
      onNoRepo()
      return
    }
    m.openInEditor.mutate(defaultEditor)
  }

  if (!hasSolutions) {
    return (
      <TooltipButton
        onClick={openEditor}
        tooltip={`Open in ${label}`}
        className={cn(buttonClass, 'w-8 rounded-md')}
      >
        <EditorGlyph kind={defaultEditor} />
      </TooltipButton>
    )
  }

  return (
    <div className="flex">
      <TooltipButton
        onClick={openEditor}
        tooltip={`Open in ${label}`}
        className={cn(buttonClass, 'w-8 rounded-l-md border-r-0')}
      >
        <EditorWithSolutionGlyph kind={defaultEditor} />
      </TooltipButton>
      <Tooltip>
        <DropdownMenu>
          <TooltipTrigger asChild>
            <DropdownMenuTrigger asChild>
              <button
                aria-label="Choose what to open"
                className={cn(buttonClass, 'w-[18px] rounded-r-md')}
              >
                <ChevronDown size={12} strokeWidth={2.2} />
              </button>
            </DropdownMenuTrigger>
          </TooltipTrigger>
          <TooltipContent>Choose what to open</TooltipContent>
          <DropdownMenuContent align="end" className="w-64">
            <DropdownMenuItem onSelect={openEditor}>
              <EditorGlyph kind={defaultEditor} size={14} className="flex-none" />
              {label}
            </DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuLabel className="flex items-center gap-1.5 text-xs text-sub">
              <VisualStudioGlyph size={12} className="flex-none" />
              {visualStudio}
            </DropdownMenuLabel>
            {solutions.map((solution: SolutionFile) => (
              <DropdownMenuItem
                key={solution.absolute_path}
                onSelect={() => {
                  if (!repo) {
                    onNoRepo()
                    return
                  }
                  m.openSolution.mutate(solution.absolute_path)
                }}
              >
                <span className="truncate">{solution.name}</span>
                {/* Two solutions can share a name in different folders, so the
                    folder is shown whenever the solution is not at the root. */}
                {solution.relative_path !== solution.name && (
                  <span className="ml-auto truncate pl-2 text-2xs text-sub">
                    {solution.relative_path.slice(0, -solution.name.length).replace(/\/$/, '')}
                  </span>
                )}
              </DropdownMenuItem>
            ))}
          </DropdownMenuContent>
        </DropdownMenu>
      </Tooltip>
    </div>
  )
}
