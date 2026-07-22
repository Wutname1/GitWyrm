import { EyeOff } from 'lucide-react'
import {
  ContextMenuSeparator,
  ContextMenuSub,
  ContextMenuSubContent,
  ContextMenuSubTrigger,
} from '@/components/ui/context-menu'
import { PendingMenuItem } from '@/components/ui/pending-menu-item'
import { useGitMutations } from '@/hooks/useGitMutations'
import { useActiveRepo } from '@/stores/workspaceStore'

interface IgnoreChoice {
  /** The literal line written to .gitignore. */
  pattern: string
  /** Menu wording and the toast, in the user's terms. */
  label: string
}

/**
 * Builds the ignore choices for a path.
 *
 * A file offers itself, each folder above it, and its extension. A folder
 * offers itself and each folder above it. Paths are matched from the repository
 * root with a leading slash so "test/test.ps1" does not also ignore
 * "src/test/test.ps1".
 */
export function ignoreChoices(path: string, isFolder: boolean): IgnoreChoice[] {
  const normalized = path.replaceAll('\\', '/').replace(/\/+$/, '')
  const segments = normalized.split('/').filter(Boolean)
  if (segments.length === 0) return []

  const choices: IgnoreChoice[] = []
  const name = segments[segments.length - 1]

  if (isFolder) {
    choices.push({ pattern: `/${normalized}/`, label: `folder ${name}/` })
  } else {
    choices.push({ pattern: `/${normalized}`, label: name })
  }

  // Every parent folder, nearest first.
  const parents = segments.slice(0, -1)
  for (let i = parents.length; i > 0; i--) {
    const folder = parents.slice(0, i).join('/')
    choices.push({ pattern: `/${folder}/`, label: `folder ${folder}/` })
  }

  if (!isFolder) {
    const dot = name.lastIndexOf('.')
    // A leading dot is the whole name (".gitignore"), not an extension.
    if (dot > 0 && dot < name.length - 1) {
      const extension = name.slice(dot)
      choices.push({ pattern: `*${extension}`, label: `every ${extension} file` })
    }
  }

  return choices
}

interface IgnoreMenuItemsProps {
  path: string
  isFolder: boolean
  disabled?: boolean
}

/**
 * The "Ignore" submenu shared by the file and folder context menus in pending
 * changes. Each choice appends one line to the repository's .gitignore.
 */
export function IgnoreMenuItems({ path, isFolder, disabled }: IgnoreMenuItemsProps) {
  const repo = useActiveRepo()
  const m = useGitMutations(repo?.id ?? null)
  const choices = ignoreChoices(path, isFolder)
  if (choices.length === 0) return null

  return (
    <>
      <ContextMenuSeparator />
      <ContextMenuSub>
        <ContextMenuSubTrigger disabled={disabled}>
          <EyeOff />
          Ignore
        </ContextMenuSubTrigger>
        <ContextMenuSubContent className="w-60">
          {choices.map((choice) => (
            <PendingMenuItem
              key={choice.pattern}
              icon={<EyeOff />}
              label={`Ignore ${choice.label}`}
              pendingLabel="Adding to .gitignore…"
              pending={
                m.addToGitignore.isPending &&
                m.addToGitignore.variables?.pattern === choice.pattern
              }
              disabled={disabled || m.addToGitignore.isPending}
              onRun={() => m.addToGitignore.mutate(choice)}
            />
          ))}
        </ContextMenuSubContent>
      </ContextMenuSub>
    </>
  )
}
