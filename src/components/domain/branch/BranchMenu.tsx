import type { BranchInfo } from '@/lib/bindings'
import { useBranches } from '@/hooks/useGitQueries'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo } from '@/stores/workspaceStore'
import { BranchMenuItems } from './BranchMenuItems'

interface BranchMenuProps {
  /** The branch to act on, or its name -- looked up when only a name is known. */
  branch: BranchInfo | string
  opInProgress?: boolean
}

/**
 * The branch menu, wired to the app's stores.
 *
 * Surfaces render this and get the whole set: the sidebar row, a graph chip,
 * a commit at a branch tip, and the remote branch row that tracks it. Rename
 * and delete open their dialogs through the ui store, so the dialogs live in
 * one place instead of once per menu.
 *
 * Renders nothing when the name doesn't resolve to a local branch -- a remote
 * branch with no local counterpart has nothing local to act on.
 */
export function BranchMenu({ branch, opInProgress }: BranchMenuProps) {
  const repo = useActiveRepo()
  const branches = useBranches(repo?.id ?? null)
  const openMerge = useUiStore((s) => s.openMerge)
  const renameBranchPrompt = useUiStore((s) => s.renameBranchPrompt)
  const deleteBranchPrompt = useUiStore((s) => s.deleteBranchPrompt)

  const resolved =
    typeof branch === 'string'
      ? branches.data?.local.find((b) => b.name === branch)
      : branch
  if (!resolved) return null

  const currentBranch = branches.data?.local.find((b) => b.is_head)?.name ?? ''

  return (
    <BranchMenuItems
      branch={resolved}
      repoId={repo?.id ?? null}
      currentBranch={currentBranch}
      opInProgress={opInProgress}
      handlers={{
        onMerge: openMerge,
        onRename: renameBranchPrompt,
        onDelete: deleteBranchPrompt,
      }}
    />
  )
}
