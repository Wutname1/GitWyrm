import type { ReactNode } from 'react'
import { Copy, GitBranchPlus, Info } from 'lucide-react'
import { toast } from 'sonner'
import type { CommitEntry } from '@/lib/bindings'
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuLabel,
  ContextMenuSeparator,
  ContextMenuShortcut,
  ContextMenuTrigger,
} from '@/components/ui/context-menu'
import { useBranches, useMergeState } from '@/hooks/useGitQueries'
import { useGitMutations } from '@/hooks/useGitMutations'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo } from '@/stores/workspaceStore'

interface CommitContextMenuProps {
  commit: CommitEntry
  onViewDetails: () => void
  children: ReactNode
}

export function CommitContextMenu({ commit, onViewDetails, children }: CommitContextMenuProps) {
  const repo = useActiveRepo()
  const branches = useBranches(repo?.id ?? null)
  const mergeState = useMergeState(repo?.id ?? null)
  const m = useGitMutations(repo?.id ?? null)

  const current = branches.data?.local.find((b) => b.is_head)
  const opInProgress = mergeState.data?.merging ?? false
  // Picking a commit that HEAD already points at is a no-op.
  const isHead = commit.refs.some((r) => r.type === 'head')
  const canCherryPick = !opInProgress && !isHead

  const copySha = () => {
    void navigator.clipboard
      .writeText(commit.sha)
      .then(() => toast(`Copied ${commit.short_sha}`))
      .catch(() => toast.error('Could not copy SHA'))
  }

  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>{children}</ContextMenuTrigger>
      <ContextMenuContent className="w-56">
        <ContextMenuLabel className="font-mono text-[11px] text-sub">
          {commit.short_sha}
        </ContextMenuLabel>
        <ContextMenuSeparator />
        <ContextMenuItem onSelect={onViewDetails}>
          <Info />
          View details
        </ContextMenuItem>
        <ContextMenuItem onSelect={copySha}>
          <Copy />
          Copy SHA
          <ContextMenuShortcut className="font-mono">{commit.short_sha}</ContextMenuShortcut>
        </ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem
          disabled={!canCherryPick}
          onSelect={() => m.cherryPick.mutate(commit.sha)}
        >
          <GitBranchPlus />
          Cherry-pick onto {current?.name ?? 'current'}
        </ContextMenuItem>
      </ContextMenuContent>
    </ContextMenu>
  )
}
