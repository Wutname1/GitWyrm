import { ArrowDown, ArrowUp } from 'lucide-react'
import type { BranchInfo } from '@/lib/bindings'
import { ContextMenuItem } from '@/components/ui/context-menu'
import { PendingIndicator } from '@/components/ui/pending-indicator'
import { useGitMutations } from '@/hooks/useGitMutations'
import { branchActions } from '@/lib/branchActions'

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
  const actions = branchActions(branch)

  const isPushing = m.pushBranch.isPending && m.pushBranch.variables === branch.name
  const isPulling = m.pullBranch.isPending && m.pullBranch.variables === branch.name
  const busy = m.pushBranch.isPending || m.pullBranch.isPending

  return (
    <>
      {actions.push.show && (
        <ContextMenuItem
          disabled={opInProgress || busy}
          onSelect={(e) => {
            e.preventDefault()
            m.pushBranch.mutate(branch.name)
          }}
        >
          {isPushing ? <PendingIndicator /> : <ArrowUp />}
          {isPushing ? 'Sending…' : actions.push.label}
        </ContextMenuItem>
      )}
      {actions.pull.show && (
        <ContextMenuItem
          disabled={opInProgress || busy}
          onSelect={(e) => {
            e.preventDefault()
            m.pullBranch.mutate(branch.name)
          }}
        >
          {isPulling ? <PendingIndicator /> : <ArrowDown />}
          {isPulling ? 'Getting…' : actions.pull.label}
        </ContextMenuItem>
      )}
    </>
  )
}

/** Whether the shared items would render anything for this branch. */
export function hasRemoteItems(branch: BranchInfo): boolean {
  const a = branchActions(branch)
  return a.push.show || a.pull.show
}
