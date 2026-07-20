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
 * The push/pull menu entries for one branch. Every menu that acts on a branch
 * renders this rather than its own copy, so the wording, the pending states and
 * the rules for when an action applies stay identical wherever you right-click.
 *
 * Renders nothing when the branch has nothing to send or get.
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
