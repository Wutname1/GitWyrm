import type { BranchInfo } from '@/lib/bindings'
import type { PreviewMode } from '@/lib/syncPreview'
import { useBranches } from '@/hooks/useGitQueries'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo } from '@/stores/workspaceStore'
import { BranchMenuItems } from './BranchMenuItems'

interface BranchMenuProps {
  /** The branch to act on, or its name -- looked up when only a name is known. */
  branch: BranchInfo | string
  opInProgress?: boolean
  /** Remote rows provide their own exact host link. */
  showWebLink?: boolean
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
export function BranchMenu({ branch, opInProgress, showWebLink = true }: BranchMenuProps) {
  const repo = useActiveRepo()
  const branches = useBranches(repo?.id ?? null)
  const openRemoteSync = useUiStore((s) => s.openRemoteSync)
  const renameBranchPrompt = useUiStore((s) => s.renameBranchPrompt)
  const deleteBranchPrompt = useUiStore((s) => s.deleteBranchPrompt)

  const resolved =
    typeof branch === 'string'
      ? branches.data?.local.find((b) => b.name === branch)
      : branch
  if (!resolved) return null

  const currentBranch = branches.data?.local.find((b) => b.is_head)?.name ?? ''

  // Every combining action opens the same window dragging one chip onto another
  // does, so blend / stack / replace are offered together with the resulting
  // shape drawn, instead of each menu item quietly picking one.
  //
  // Argument order matches a drag: the FIRST ref is the one "picked up", and
  // for two local branches that is the source whose commits travel. The menu
  // reads "merge <other> into <current>", so the other branch goes first.
  const onMerge = (name: string, mode?: PreviewMode) =>
    openRemoteSync(name, currentBranch, mode)

  return (
    <BranchMenuItems
      branch={resolved}
      repoId={repo?.id ?? null}
      currentBranch={currentBranch}
      opInProgress={opInProgress}
      showWebLink={showWebLink}
      handlers={{
        onMerge,
        onRename: renameBranchPrompt,
        onDelete: deleteBranchPrompt,
      }}
    />
  )
}
