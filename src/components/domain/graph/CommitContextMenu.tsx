import { type ReactNode, useState } from 'react'
import { Copy, ExternalLink, GitBranchPlus, Info, MoveVertical, RotateCcw, Tag } from 'lucide-react'
import { toast } from 'sonner'
import type { CommitEntry, ResetMode } from '@/lib/bindings'
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuLabel,
  ContextMenuSeparator,
  ContextMenuShortcut,
  ContextMenuSub,
  ContextMenuSubContent,
  ContextMenuSubTrigger,
  ContextMenuTrigger,
} from '@/components/ui/context-menu'
import { ConfirmDialog } from '@/components/modals/ConfirmDialog'
import { useBranches, useMergeState } from '@/hooks/useGitQueries'
import { useGitMutations } from '@/hooks/useGitMutations'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo } from '@/stores/workspaceStore'

interface CommitContextMenuProps {
  commit: CommitEntry
  onViewDetails: () => void
  children: ReactNode
}

type Pending = { kind: 'reset'; mode: ResetMode } | { kind: 'move' } | null

export function CommitContextMenu({ commit, onViewDetails, children }: CommitContextMenuProps) {
  const repo = useActiveRepo()
  const branches = useBranches(repo?.id ?? null)
  const mergeState = useMergeState(repo?.id ?? null)
  const m = useGitMutations(repo?.id ?? null)
  const openNewTag = useUiStore((s) => s.openNewTag)
  const [pending, setPending] = useState<Pending>(null)

  const current = branches.data?.local.find((b) => b.is_head)
  const branchName = current?.name ?? 'current'
  const opInProgress = mergeState.data?.merging ?? false
  const isHead = commit.refs.some((r) => r.type === 'head')
  const canCherryPick = !opInProgress && !isHead
  // Moving/resetting to where the branch already is would be a no-op.
  const canRetarget = !opInProgress && !isHead && current != null

  const copySha = () => {
    void navigator.clipboard
      .writeText(commit.sha)
      .then(() => toast(`Copied ${commit.short_sha}`))
      .catch(() => toast.error('Could not copy SHA'))
  }

  const runReset = (mode: ResetMode) => {
    // Soft/Mixed keep the working tree, so they run without a gate. Hard
    // rewrites the working tree and can drop work: confirm first.
    if (mode === 'Hard') setPending({ kind: 'reset', mode })
    else m.reset.mutate({ sha: commit.sha, mode })
  }

  return (
    <>
      <ContextMenu>
        <ContextMenuTrigger asChild>{children}</ContextMenuTrigger>
        <ContextMenuContent className="w-60">
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
          <ContextMenuItem onSelect={() => m.openOnGitHub.mutate(commit.sha)}>
            <ExternalLink />
            Open on GitHub
          </ContextMenuItem>
          <ContextMenuItem onSelect={() => openNewTag(commit.sha)}>
            <Tag />
            Tag this commit
          </ContextMenuItem>
          <ContextMenuSeparator />
          <ContextMenuItem disabled={!canCherryPick} onSelect={() => m.cherryPick.mutate(commit.sha)}>
            <GitBranchPlus />
            Cherry-pick onto {branchName}
          </ContextMenuItem>
          <ContextMenuItem disabled={!canRetarget} onSelect={() => setPending({ kind: 'move' })}>
            <MoveVertical />
            Move {branchName} to this commit
          </ContextMenuItem>
          <ContextMenuSub>
            <ContextMenuSubTrigger
              className="data-[disabled]:pointer-events-none data-[disabled]:opacity-50"
              data-disabled={!canRetarget ? '' : undefined}
            >
              <RotateCcw />
              Rewind {branchName} to here
            </ContextMenuSubTrigger>
            <ContextMenuSubContent className="w-64">
              <ContextMenuItem onSelect={() => runReset('Mixed')}>
                <div className="flex flex-col">
                  <span>Undo the later commits</span>
                  <span className="text-[10px] text-muted-foreground">
                    Keeps their changes in your files
                  </span>
                </div>
              </ContextMenuItem>
              <ContextMenuItem onSelect={() => runReset('Soft')}>
                <div className="flex flex-col">
                  <span>Undo, and keep changes ready to commit</span>
                  <span className="text-[10px] text-muted-foreground">
                    Changes stay staged
                  </span>
                </div>
              </ContextMenuItem>
              <ContextMenuItem variant="destructive" onSelect={() => runReset('Hard')}>
                <div className="flex flex-col">
                  <span>Undo and erase the later changes</span>
                  <span className="text-[10px] opacity-80">Can't be undone easily</span>
                </div>
              </ContextMenuItem>
            </ContextMenuSubContent>
          </ContextMenuSub>
        </ContextMenuContent>
      </ContextMenu>

      <ConfirmDialog
        open={pending?.kind === 'reset'}
        onOpenChange={(o) => !o && setPending(null)}
        destructive
        title={`Erase changes on ${branchName}?`}
        description={
          <>
            This rewinds <span className="font-mono text-foreground">{branchName}</span> to{' '}
            <span className="font-mono text-foreground">{commit.short_sha}</span> and{' '}
            <span className="text-removed">erases any work you haven't committed</span>. The commits
            made after this point will also be removed. This is hard to undo.
          </>
        }
        confirmLabel="Erase and rewind"
        confirmPhrase={branchName}
        onConfirm={() => m.reset.mutate({ sha: commit.sha, mode: 'Hard' })}
      />

      <ConfirmDialog
        open={pending?.kind === 'move'}
        onOpenChange={(o) => !o && setPending(null)}
        destructive
        title={`Move ${branchName} to this commit?`}
        description={
          <>
            This moves <span className="font-mono text-foreground">{branchName}</span> to{' '}
            <span className="font-mono text-foreground">{commit.short_sha}</span>. The commits made
            after this point will be removed from the branch. You can undo this right after.
          </>
        }
        confirmLabel="Move branch"
        onConfirm={() => m.moveBranch.mutate(commit.sha)}
      />
    </>
  )
}
