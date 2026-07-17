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
import { useMergeState } from '@/hooks/useGitQueries'
import { useGitMutations } from '@/hooks/useGitMutations'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo } from '@/stores/workspaceStore'

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
  const m = useGitMutations(repo?.id ?? null)
  const openMerge = useUiStore((s) => s.openMerge)

  const isBranch = refTag.type === 'branch'
  if (!isBranch) return <>{children}</>

  const opInProgress = mergeState.data?.merging ?? false

  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>{children}</ContextMenuTrigger>
      <ContextMenuContent className="w-52">
        <ContextMenuLabel className="font-mono text-[11px] text-sub">{refTag.name}</ContextMenuLabel>
        <ContextMenuSeparator />
        <ContextMenuItem disabled={opInProgress} onSelect={() => m.checkout.mutate(refTag.name)}>
          <GitBranch />
          Switch to {refTag.name}
        </ContextMenuItem>
        <ContextMenuItem disabled={opInProgress} onSelect={() => openMerge(refTag.name)}>
          <GitMerge />
          Combine with this branch…
        </ContextMenuItem>
      </ContextMenuContent>
    </ContextMenu>
  )
}
