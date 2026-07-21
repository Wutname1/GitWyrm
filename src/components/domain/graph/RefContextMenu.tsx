import type { ReactNode } from 'react'
import type { RefInfo } from '@/lib/bindings'
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuLabel,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from '@/components/ui/context-menu'
import { useMergeState } from '@/hooks/useGitQueries'
import { useActiveRepo } from '@/stores/workspaceStore'
import { BranchMenu } from '@/components/domain/branch/BranchMenu'

interface RefContextMenuProps {
  refTag: RefInfo
  children: ReactNode
}

/**
 * Right-click menu for a branch chip in the graph. Offers the same actions as
 * every other branch surface via BranchMenu; remote chips and tags fall through
 * to a plain (menu-less) chip.
 *
 * The checked-out branch's chip is tagged `head`, not `branch` -- both are
 * local branches and both get this menu.
 */
export function RefContextMenu({ refTag, children }: RefContextMenuProps) {
  const repo = useActiveRepo()
  const mergeState = useMergeState(repo?.id ?? null)

  const isBranch = refTag.type === 'branch' || refTag.type === 'head'
  if (!isBranch) return <>{children}</>

  const opInProgress = mergeState.data?.merging ?? false

  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>{children}</ContextMenuTrigger>
      <ContextMenuContent className="z-[70] w-60">
        <ContextMenuLabel className="font-mono text-2xs text-sub">{refTag.name}</ContextMenuLabel>
        <ContextMenuSeparator />
        <BranchMenu branch={refTag.name} opInProgress={opInProgress} />
      </ContextMenuContent>
    </ContextMenu>
  )
}
