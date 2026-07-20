import type { ReactNode } from 'react'
import { GitBranch, GitMerge } from 'lucide-react'
import type { RefInfo } from '@/lib/bindings'
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuLabel,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from '@/components/ui/context-menu'
import { useBranches, useMergeState } from '@/hooks/useGitQueries'
import { BranchRemoteItems, hasRemoteItems } from '@/components/domain/branch/BranchRemoteItems'
import { useGitMutations } from '@/hooks/useGitMutations'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo } from '@/stores/workspaceStore'
import { PendingIndicator } from '@/components/ui/pending-indicator'

interface RefContextMenuProps {
  refTag: RefInfo
  children: ReactNode
}

/**
 * Right-click menu for a branch chip. Actions target the clicked branch: it is
 * always the OTHER side of the operation, so directions are unambiguous.
 * Only local, non-current branches get actions today; the current HEAD and
 * remote/tag chips fall through to a plain (menu-less) chip.
 */
export function RefContextMenu({ refTag, children }: RefContextMenuProps) {
  const repo = useActiveRepo()
  const mergeState = useMergeState(repo?.id ?? null)
  const branches = useBranches(repo?.id ?? null)
  const m = useGitMutations(repo?.id ?? null)
  const openMerge = useUiStore((s) => s.openMerge)

  // The checked-out branch's chip is tagged `head`, not `branch`. Both are
  // local branches and both get this menu; remote chips and tags do not.
  const isBranch = refTag.type === 'branch' || refTag.type === 'head'
  if (!isBranch) return <>{children}</>

  const opInProgress = mergeState.data?.merging ?? false

  // The chip knows only the ref name, so read sync state from the branch query.
  const branch = branches.data?.local.find((b) => b.name === refTag.name)
  const isCurrent = branch?.is_head ?? false
  const hasActions = branch ? hasRemoteItems(branch) : false

  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>{children}</ContextMenuTrigger>
      <ContextMenuContent className="w-56">
        <ContextMenuLabel className="font-mono text-[11px] text-sub">{refTag.name}</ContextMenuLabel>
        <ContextMenuSeparator />
        {/* Switching to, or merging in, the branch you are already on is a
            no-op, so the current branch gets only its remote actions. */}
        {!isCurrent && (
          <>
            <ContextMenuItem
              disabled={opInProgress || m.checkout.isPending}
              onSelect={(e) => {
                e.preventDefault()
                m.checkout.mutate(refTag.name)
              }}
            >
              {m.checkout.isPending ? <PendingIndicator /> : <GitBranch />}
              {m.checkout.isPending ? `Switching to ${refTag.name}…` : `Switch to ${refTag.name}`}
            </ContextMenuItem>
            <ContextMenuItem
              disabled={opInProgress || m.checkout.isPending}
              onSelect={() => openMerge(refTag.name)}
            >
              <GitMerge />
              Combine with this branch…
            </ContextMenuItem>
          </>
        )}
        {branch && (
          <BranchRemoteItems branch={branch} repoId={repo?.id ?? null} opInProgress={opInProgress} />
        )}
        {/* The branch you are on, with nothing to send or get, has no action
            left -- say so rather than showing a menu that is only a label. */}
        {isCurrent && !hasActions && (
          <ContextMenuItem disabled>This branch matches the remote</ContextMenuItem>
        )}
      </ContextMenuContent>
    </ContextMenu>
  )
}
