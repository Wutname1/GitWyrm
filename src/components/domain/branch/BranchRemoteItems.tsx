import { ArrowDown, ArrowUp } from 'lucide-react'
import type { BranchInfo } from '@/lib/bindings'
import { PendingMenuItem } from '@/components/ui/pending-menu-item'
import { useGitMutations } from '@/hooks/useGitMutations'
import { useBranchHost } from '@/hooks/useGitQueries'
import { branchActions, branchSync } from '@/lib/branchActions'
import { useUiStore } from '@/stores/uiStore'

interface BranchRemoteItemsProps {
  branch: BranchInfo
  repoId: string | null
  /** Set while a merge or similar is mid-flight, which blocks remote work. */
  opInProgress?: boolean
}

/**
 * Just the push/pull entries for a branch, for menus that are already long.
 *
 * The commit menu carries fifteen commit actions of its own, so a branch at a
 * commit gets only the two that matter there. Every other surface renders the
 * full [`BranchMenu`] instead.
 */
export function BranchRemoteItems({ branch, repoId, opInProgress }: BranchRemoteItemsProps) {
  const m = useGitMutations(repoId)
  const host = useBranchHost(repoId, branch.upstream)
  const actions = branchActions(branch, host)

  const isPushing = m.pushBranch.isPending && m.pushBranch.variables === branch.name
  const isPulling = m.pullBranch.isPending && m.pullBranch.variables === branch.name
  const busy = m.pushBranch.isPending || m.pullBranch.isPending || m.pushBranchForce.isPending
  const openPushChoice = useUiStore((s) => s.openPushChoice)
  // Behind its cloud copy (after a rebase, say): a plain push is refused, so ask
  // whether to get those changes first or force push.
  const pushOrAsk = () =>
    branchSync(branch).behind > 0 ? openPushChoice(branch.name) : m.pushBranch.mutate(branch.name)

  return (
    <>
      {actions.push.show && (
        <PendingMenuItem
          icon={<ArrowUp />}
          label={actions.push.label}
          pendingLabel="Sending…"
          pending={isPushing}
          disabled={opInProgress || busy}
          onRun={pushOrAsk}
        />
      )}
      {actions.pull.show && (
        <PendingMenuItem
          icon={<ArrowDown />}
          label={actions.pull.label}
          pendingLabel="Getting…"
          pending={isPulling}
          disabled={opInProgress || busy}
          onRun={() => m.pullBranch.mutate(branch.name)}
        />
      )}
    </>
  )
}

/** Whether the shared items would render anything for this branch. */
export function hasRemoteItems(branch: BranchInfo): boolean {
  const a = branchActions(branch)
  return a.push.show || a.pull.show
}
