import { type ReactNode, useState } from 'react'
import {
  Copy,
  CornerLeftUp,
  ExternalLink,
  GitBranchPlus,
  Info,
  Link as LinkIcon,
  LogIn,
  MoveVertical,
  Pencil,
  RotateCcw,
  Tag,
  Trash2,
  Undo2,
} from 'lucide-react'
import { toast } from 'sonner'
import { copyToClipboard } from '@/lib/clipboard'
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
import { RewordDialog } from '@/components/modals/RewordDialog'
import { PendingIndicator } from '@/components/ui/pending-indicator'
import { PendingMenuItem } from '@/components/ui/pending-menu-item'
import { BranchRemoteItems, hasRemoteItems } from '@/components/domain/branch/BranchRemoteItems'
import { useBranches, useCommitDetail, useMergeState } from '@/hooks/useGitQueries'
import { useGitMutations } from '@/hooks/useGitMutations'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo } from '@/stores/workspaceStore'

interface CommitContextMenuProps {
  commit: CommitEntry
  onViewDetails: () => void
  children: ReactNode
}

type Pending =
  | { kind: 'move' }
  | { kind: 'checkout' }
  | { kind: 'drop' }
  | { kind: 'reword' }
  | null

export function CommitContextMenu({ commit, onViewDetails, children }: CommitContextMenuProps) {
  const repo = useActiveRepo()
  const branches = useBranches(repo?.id ?? null)
  const mergeState = useMergeState(repo?.id ?? null)
  const m = useGitMutations(repo?.id ?? null)
  const openNewTag = useUiStore((s) => s.openNewTag)
  const openNewBranch = useUiStore((s) => s.openNewBranch)
  const revealShaInGraph = useUiStore((s) => s.revealShaInGraph)
  const [pending, setPending] = useState<Pending>(null)

  // Only fetched when the reword dialog opens, so most right-clicks cost nothing.
  const detail = useCommitDetail(repo?.id ?? null, pending?.kind === 'reword' ? commit.sha : null)

  const current = branches.data?.local.find((b) => b.is_head)
  const branchName = current?.name ?? 'current'
  const opInProgress = mergeState.data?.merging ?? false
  const isHead = commit.refs.some((r) => r.type === 'head')
  const historyPending =
    m.cherryPick.isPending ||
    m.reset.isPending ||
    m.moveBranch.isPending ||
    m.revertCommit.isPending ||
    m.dropCommit.isPending
  const canCherryPick = !opInProgress && !isHead && !historyPending
  // Moving/resetting to where the branch already is would be a no-op.
  const canRetarget = !opInProgress && !isHead && current != null && !historyPending
  // Revert applies on top of HEAD, so it works on any commit including HEAD.
  const canRevert = !opInProgress && current != null && !historyPending
  // Dropping rewrites history below the commit; needs a branch and a clean-ish state.
  const canDrop = !opInProgress && current != null && !historyPending

  // A commit can be the tip of more than one branch. Offer remote actions for
  // each that has something to send or get, so the common case -- right-clicking
  // the newest commit -- reaches push without hunting for the branch chip.
  // The checked-out branch's ref is tagged `head`, not `branch`, so match both
  // or the current branch -- the one most likely to have commits to send -- is
  // the only one that never gets these actions.
  const tipBranches = (branches.data?.local ?? [])
    .filter((b) =>
      commit.refs.some((r) => (r.type === 'branch' || r.type === 'head') && r.name === b.name)
    )
    .filter(hasRemoteItems)

  const copySha = () => void copyToClipboard(commit.sha, `Copied ${commit.short_sha}`)

  // Only Soft/Mixed run from here -- both keep every change on disk, so there
  // is nothing to lose and nothing to confirm. Sending the files back too is
  // "Go to this commit", which has its own confirm.
  const runReset = (mode: ResetMode) => m.reset.mutate({ sha: commit.sha, mode })

  return (
    <>
      <ContextMenu>
        <ContextMenuTrigger asChild>{children}</ContextMenuTrigger>
        <ContextMenuContent className="w-64">
          <ContextMenuLabel className="font-mono text-2xs text-sub">
            {commit.short_sha}
          </ContextMenuLabel>
          <ContextMenuSeparator />
          {tipBranches.map((branch) => (
            <BranchRemoteItems
              key={branch.name}
              branch={branch}
              repoId={repo?.id ?? null}
              opInProgress={opInProgress}
            />
          ))}
          {tipBranches.length > 0 && <ContextMenuSeparator />}
          <ContextMenuItem onSelect={onViewDetails}>
            <Info />
            View details
          </ContextMenuItem>
          {/* A merge has more than one parent, so each is offered by sha. The
              first parent is the branch the merge was made on, which is the one
              people usually mean by "the previous commit". */}
          {commit.parent_shas.length === 1 ? (
            <ContextMenuItem onSelect={() => revealShaInGraph(commit.parent_shas[0])}>
              <CornerLeftUp />
              Go to previous commit
            </ContextMenuItem>
          ) : commit.parent_shas.length > 1 ? (
            <ContextMenuSub>
              <ContextMenuSubTrigger>
                <CornerLeftUp />
                Go to previous commit
              </ContextMenuSubTrigger>
              <ContextMenuSubContent>
                {commit.parent_shas.map((sha, i) => (
                  <ContextMenuItem key={sha} onSelect={() => revealShaInGraph(sha)}>
                    <span className="font-mono">{sha.slice(0, 7)}</span>
                    <span className="text-muted-foreground">
                      {i === 0 ? 'merged into' : 'merged in'}
                    </span>
                  </ContextMenuItem>
                ))}
              </ContextMenuSubContent>
            </ContextMenuSub>
          ) : null}
          <ContextMenuItem onSelect={() => setPending({ kind: 'checkout' })}>
            <LogIn />
            Check out this commit
          </ContextMenuItem>
          <ContextMenuItem onSelect={() => openNewBranch(commit.sha)}>
            <GitBranchPlus />
            Create branch here
          </ContextMenuItem>
          <ContextMenuItem onSelect={() => openNewTag(commit.sha)}>
            <Tag />
            Tag this commit
          </ContextMenuItem>
          <ContextMenuSeparator />
          <ContextMenuItem
            disabled={m.rewordCommit.isPending || (!isHead && !canDrop)}
            onSelect={(e) => {
              e.preventDefault()
              setPending({ kind: 'reword' })
            }}
          >
            <Pencil />
            Edit commit message
          </ContextMenuItem>
          <PendingMenuItem
            icon={<Undo2 />}
            label="Undo this commit (revert)"
            pendingLabel="Undoing…"
            pending={m.revertCommit.isPending}
            disabled={!canRevert}
            onRun={() => m.revertCommit.mutate(commit.sha)}
          />
          <ContextMenuItem
            variant="destructive"
            disabled={!canDrop}
            onSelect={(e) => {
              e.preventDefault()
              setPending({ kind: 'drop' })
            }}
          >
            <Trash2 />
            Drop this commit
          </ContextMenuItem>
          <ContextMenuSeparator />
          <PendingMenuItem
            icon={<GitBranchPlus />}
            label={`Cherry-pick onto ${branchName}`}
            pendingLabel="Adding commit…"
            pending={m.cherryPick.isPending}
            disabled={!canCherryPick || m.cherryPick.isPending}
            onRun={() => m.cherryPick.mutate(commit.sha)}
          />
          <ContextMenuItem
            variant="destructive"
            disabled={!canRetarget}
            onSelect={() => setPending({ kind: 'move' })}
          >
            <MoveVertical />
            <div className="flex flex-col">
              <span>Go to this commit</span>
              <span className="text-2xs opacity-80">
                Your files end up exactly as they were here
              </span>
            </div>
          </ContextMenuItem>
          <ContextMenuSub>
            <ContextMenuSubTrigger
              className="data-[disabled]:pointer-events-none data-[disabled]:opacity-50"
              data-disabled={!canRetarget ? '' : undefined}
            >
              <RotateCcw />
              Undo the commits after this one
            </ContextMenuSubTrigger>
            <ContextMenuSubContent className="w-64">
              <ContextMenuItem
                disabled={historyPending}
                onSelect={(e) => {
                  e.preventDefault()
                  runReset('Mixed')
                }}
              >
                {m.reset.isPending && m.reset.variables?.mode === 'Mixed' && <PendingIndicator />}
                <div className="flex flex-col">
                  <span>
                    {m.reset.isPending && m.reset.variables?.mode === 'Mixed'
                      ? 'Undoing…'
                      : 'Keep their changes in my files'}
                  </span>
                  <span className="text-2xs text-muted-foreground">
                    Nothing is lost, just uncommitted
                  </span>
                </div>
              </ContextMenuItem>
              <ContextMenuItem
                disabled={historyPending}
                onSelect={(e) => {
                  e.preventDefault()
                  runReset('Soft')
                }}
              >
                {m.reset.isPending && m.reset.variables?.mode === 'Soft' && <PendingIndicator />}
                <div className="flex flex-col">
                  <span>
                    {m.reset.isPending && m.reset.variables?.mode === 'Soft'
                      ? 'Undoing…'
                      : 'Keep their changes ready to commit'}
                  </span>
                  <span className="text-2xs text-muted-foreground">Changes stay staged</span>
                </div>
              </ContextMenuItem>
            </ContextMenuSubContent>
          </ContextMenuSub>
          <ContextMenuSeparator />
          <ContextMenuItem onSelect={copySha}>
            <Copy />
            Copy SHA
            <ContextMenuShortcut className="font-mono">{commit.short_sha}</ContextMenuShortcut>
          </ContextMenuItem>
          <ContextMenuItem onSelect={() => m.copyCommitLink.mutate(commit.sha)}>
            <LinkIcon />
            Copy link to commit
          </ContextMenuItem>
          <ContextMenuItem onSelect={() => m.openOnGitHub.mutate(commit.sha)}>
            <ExternalLink />
            Open on GitHub
          </ContextMenuItem>
        </ContextMenuContent>
      </ContextMenu>

      <ConfirmDialog
        open={pending?.kind === 'checkout'}
        onOpenChange={(o) => !o && setPending(null)}
        title="Check out this commit?"
        description={
          <>
            This puts your files at <span className="font-mono text-foreground">{commit.short_sha}</span>{' '}
            but leaves you off any branch (a "detached" state). New commits here won't belong to a
            branch until you make one. To get back, switch to a branch like{' '}
            <span className="font-mono text-foreground">{branchName}</span>.
          </>
        }
        confirmLabel="Check out commit"
        pending={m.checkoutCommit.isPending}
        pendingLabel="Checking out…"
        keepOpenOnConfirm
        onConfirm={() =>
          m.checkoutCommit.mutate(commit.sha, { onSuccess: () => setPending(null) })
        }
      />

      <ConfirmDialog
        open={pending?.kind === 'drop'}
        onOpenChange={(o) => !o && setPending(null)}
        destructive
        title="Drop this commit?"
        description={
          <>
            This removes <span className="font-mono text-foreground">{commit.short_sha}</span> from{' '}
            <span className="font-mono text-foreground">{branchName}</span> and replays the commits
            after it on top of its parent. If a later commit depended on this one you may hit
            conflicts, in which case nothing is changed. You can undo this right after.
          </>
        }
        confirmLabel="Drop commit"
        pending={m.dropCommit.isPending}
        pendingLabel="Dropping…"
        keepOpenOnConfirm
        onConfirm={() =>
          m.dropCommit.mutate(commit.sha, { onSuccess: () => setPending(null) })
        }
      />

      <RewordDialog
        open={pending?.kind === 'reword'}
        onOpenChange={(o) => !o && setPending(null)}
        initialSummary={detail.data?.summary ?? commit.summary}
        initialBody={detail.data?.body ?? ''}
        pending={m.rewordCommit.isPending}
        onConfirm={(message) =>
          m.rewordCommit.mutate(
            { sha: commit.sha, message },
            { onSuccess: () => setPending(null) }
          )
        }
      />

      <ConfirmDialog
        open={pending?.kind === 'move'}
        onOpenChange={(o) => !o && setPending(null)}
        destructive
        title={`Send ${branchName} back to this commit?`}
        description={
          <>
            <span className="font-mono text-foreground">{branchName}</span> goes back to{' '}
            <span className="font-mono text-foreground">{commit.short_sha}</span>, and your files
            end up exactly as they were at that point.{' '}
            <span className="text-removed">
              The commits made after it are removed from the branch
            </span>
            . Any work you haven't committed is saved to a stash first, so nothing is lost. You can
            undo this right after.
          </>
        }
        confirmLabel="Go to this commit"
        pending={m.moveBranch.isPending}
        pendingLabel="Going back…"
        keepOpenOnConfirm
        onConfirm={() =>
          m.moveBranch.mutate(commit.sha, { onSuccess: () => setPending(null) })
        }
      />
    </>
  )
}
