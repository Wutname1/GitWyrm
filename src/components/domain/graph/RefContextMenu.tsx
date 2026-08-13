import type { ReactNode } from 'react'
import type { RefInfo } from '@/lib/bindings'
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuLabel,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from '@/components/ui/context-menu'
import { useBranches, useMergeState, useRemotes } from '@/hooks/useGitQueries'
import { useActiveRepo } from '@/stores/workspaceStore'
import { BranchMenu } from '@/components/domain/branch/BranchMenu'
import { RemoteBranchMenuItems } from '@/components/domain/branch/RemoteBranchMenuItems'

interface RefContextMenuProps {
  refTag: RefInfo
  children: ReactNode
  onOpenChange?: (open: boolean) => void
}

/**
 * Right-click menu for a branch chip in the graph. Offers the same actions as
 * every other branch surface via BranchMenu. Remote chips link to their exact
 * branch on the host; tags fall through to a plain chip.
 *
 * The checked-out branch's chip is tagged `head`, not `branch` -- both are
 * local branches and both get this menu.
 */
export function RefContextMenu({ refTag, children, onOpenChange }: RefContextMenuProps) {
  const repo = useActiveRepo()
  const mergeState = useMergeState(repo?.id ?? null)
  const remotes = useRemotes(repo?.id ?? null)
  const branches = useBranches(repo?.id ?? null)

  const isBranch = refTag.type === 'branch' || refTag.type === 'head'
  const isRemote = refTag.type === 'remote'
  if (!isBranch && !isRemote) return <>{children}</>

  // `origin/feature/x` -> remote `origin`, branch `feature/x`. Only the first
  // segment is the remote; the rest is the branch, slashes and all.
  const [remoteName, ...remoteBranchParts] = isRemote ? refTag.name.split('/') : []
  const remoteBranch = remoteBranchParts.join('/')
  const remote = remotes.data?.find((item) => item.name === remoteName)
  // Without the remote record there is nothing to act on -- fall through to a
  // plain chip rather than an empty menu.
  if (isRemote && (!remote || !remoteBranch)) return <>{children}</>

  const record = remote?.branches.find((b) => b.name === remoteBranch)
  // The remotes query knows the counterpart; fall back to a name match so the
  // menu still reads correctly before that query settles.
  const localCounterpart =
    record?.local_counterpart ??
    branches.data?.local.find((b) => b.name === remoteBranch)?.name ??
    null

  const opInProgress = mergeState.data?.merging ?? false

  return (
    <ContextMenu onOpenChange={onOpenChange}>
      <ContextMenuTrigger asChild>{children}</ContextMenuTrigger>
      <ContextMenuContent className="z-[70] w-60">
        <ContextMenuLabel className="font-mono text-2xs text-sub">{refTag.name}</ContextMenuLabel>
        <ContextMenuSeparator />
        {isBranch ? (
          <BranchMenu branch={refTag.name} opInProgress={opInProgress} />
        ) : remote ? (
          <RemoteBranchMenuItems
            remote={remote}
            branch={remoteBranch}
            repoId={repo?.id ?? null}
            localCounterpart={localCounterpart}
            trackedBy={record?.tracked_by}
            tip={record?.tip}
            opInProgress={opInProgress}
          />
        ) : null}
      </ContextMenuContent>
    </ContextMenu>
  )
}
