import { useMemo } from 'react'
import { useQuery } from '@tanstack/react-query'
import { AlertTriangle, ArrowRight, Check, Cloud, GitMerge, Repeat2, Zap } from 'lucide-react'
import { cn } from '@/lib/utils'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { commands } from '@/lib/bindings'
import { unwrap } from '@/lib/queryKeys'
import { resolveDropPair, type DropPair } from '@/lib/refSync'
import { useBranches } from '@/hooks/useGitQueries'
import { branchSync } from '@/lib/branchActions'
import { useGitMutations } from '@/hooks/useGitMutations'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo } from '@/stores/workspaceStore'

/** A ref chip styled to match the graph's pills. */
function Chip({ name, tone }: { name: string; tone: 'source' | 'target' }) {
  return (
    <span
      className={cn(
        'max-w-[150px] overflow-hidden text-ellipsis whitespace-nowrap rounded-[5px] border px-2 py-1 font-mono text-[11px] font-semibold',
        tone === 'target'
          ? 'border-primary bg-soft text-primary'
          : 'border-border bg-panel2 text-foreground'
      )}
    >
      {name || '…'}
    </span>
  )
}

/**
 * The concrete action a drop resolves to. The refs' actual relationship
 * decides - not which way the pills were dragged - so any drag offers whatever
 * would put the two in sync. Drag direction only picks the options for the
 * tracking-pair diverged case (dropping local onto remote reads as "make the
 * cloud look like me" -> offer the overwrite).
 *
 * Tracking pair (branch vs its own upstream): up-to-date / fast-forward /
 * push / diverged. Branch pair (anything else dropped on a local branch,
 * "bring the source's work into the target"): same-point / contains /
 * ff-branch / diverged-branches.
 */
type Action =
  | { kind: 'up-to-date' }
  | { kind: 'fast-forward' }
  | { kind: 'push' }
  | { kind: 'diverged-incoming' }
  | { kind: 'diverged-outgoing' }
  | { kind: 'upstream-gone' }
  | { kind: 'same-point' }
  | { kind: 'contains' }
  | { kind: 'ff-branch' }
  | { kind: 'diverged-branches' }

/**
 * Reads `sync`, not the raw counts. `SyncState::counts()` reports (0, 0) for a
 * branch whose upstream ref is gone, which is indistinguishable from a branch
 * that matches -- so the raw counts would call a stale branch "up to date".
 */
function chooseTrackingAction(pair: Extract<DropPair, { kind: 'tracking' }>): Action {
  const sync = pair.branch.sync
  switch (sync.kind) {
    case 'in_sync':
      return { kind: 'up-to-date' }
    case 'upstream_gone':
      return { kind: 'upstream-gone' }
    // A pair only resolves when the branch has an upstream configured, so a
    // never-pushed branch cannot reach here -- it is filtered by
    // resolveSyncPair. Kept explicit so the switch stays exhaustive.
    case 'never_pushed':
      return { kind: 'up-to-date' }
    case 'diverged': {
      const { ahead, behind } = sync
      if (behind > 0 && ahead === 0) return { kind: 'fast-forward' }
      if (ahead > 0 && behind === 0) return { kind: 'push' }
      return pair.direction === 'outgoing'
        ? { kind: 'diverged-outgoing' }
        : { kind: 'diverged-incoming' }
    }
  }
}

/** relation = target vs source: ahead = target-only commits, behind = source-only. */
function chooseBranchAction(relation: { ahead: number; behind: number }): Action {
  if (relation.ahead === 0 && relation.behind === 0) return { kind: 'same-point' }
  if (relation.behind === 0) return { kind: 'contains' }
  if (relation.ahead === 0) return { kind: 'ff-branch' }
  return { kind: 'diverged-branches' }
}

export function RemoteSyncModal() {
  const open = useUiStore((s) => s.activeModal === 'remote-sync')
  const closeModal = useUiStore((s) => s.closeModal)
  const openConflict = useUiStore((s) => s.openConflict)
  const syncSource = useUiStore((s) => s.syncSource)
  const syncTarget = useUiStore((s) => s.syncTarget)

  const repo = useActiveRepo()
  const branches = useBranches(repo?.id ?? null)
  const m = useGitMutations(repo?.id ?? null)

  const pair = useMemo<DropPair | null>(() => {
    if (!syncSource || !syncTarget || !branches.data) return null
    // A name matching a local branch is local (even if it contains a slash,
    // like `feature/foo`); otherwise it's the remote-tracking ref.
    const t = (name: string): 'remote' | 'head' | 'branch' => {
      const local = branches.data!.local.find((b) => b.name === name)
      if (!local) return 'remote'
      return local.is_head ? 'head' : 'branch'
    }
    return resolveDropPair(
      { name: syncSource, type: t(syncSource) },
      { name: syncTarget, type: t(syncTarget) },
      branches.data
    )
  }, [syncSource, syncTarget, branches.data])

  const branchPair = pair?.kind === 'branches' ? pair : null

  // For a branch pair, how the target relates to the source (ahead = commits
  // only the target has, behind = commits only the source has).
  const relation = useQuery({
    queryKey: ['branchRelation', repo?.id, branchPair?.target.name, branchPair?.source.name],
    enabled: open && !!repo && !!branchPair,
    queryFn: async () =>
      unwrap(await commands.branchRelation(repo!.id, branchPair!.target.name, branchPair!.source.name)),
  })

  const action: Action | null =
    pair?.kind === 'tracking'
      ? chooseTrackingAction(pair)
      : branchPair && relation.data
        ? chooseBranchAction(relation.data)
        : null

  // Names used in the copy. For a branch pair, "bring source's work into target".
  const srcName = pair?.kind === 'tracking' ? pair.upstream : (branchPair?.source.name ?? '')
  const tgtName = pair?.kind === 'tracking' ? pair.branch.name : (branchPair?.target.name ?? '')

  // Counts for the copy below. Only meaningful for a diverged tracking pair,
  // which is the only state that reaches the messages using them.
  const trackingSync =
    pair?.kind === 'tracking' ? branchSync(pair.branch) : { ahead: 0, behind: 0 }

  // The chips show where commits will actually flow, which may be the reverse
  // of the drag: dropping origin/main onto main while you're ahead means a
  // push, so the picture reads main -> origin/main.
  const commitsGoUp = action?.kind === 'push' || action?.kind === 'diverged-outgoing'
  const flowFrom = pair ? (commitsGoUp ? tgtName : srcName) : syncSource
  const flowInto = pair ? (commitsGoUp ? srcName : tgtName) : syncTarget

  // Moving a branch that isn't checked out switches to it first.
  const headName = branches.data?.local.find((b) => b.is_head)?.name
  const switchesBranch = !!branchPair && branchPair.target.name !== headName

  const pending =
    m.pull.isPending ||
    m.push.isPending ||
    m.pushBranch.isPending ||
    m.pushForce.isPending ||
    m.rebase.isPending ||
    m.mergeDirectional.isPending

  const onConflicts = (conflicts: string[]) => {
    closeModal()
    if (conflicts.length > 0) openConflict(conflicts[0])
  }

  const runPull = () => m.pull.mutate(undefined, { onSuccess: () => closeModal() })
  const runPush = () => m.push.mutate(undefined, { onSuccess: () => closeModal() })
  // Republishing a branch whose cloud copy was deleted: the branch need not be
  // the checked-out one, so this goes through the branch-aware push, which also
  // re-links the upstream.
  const runRepublish = () => {
    if (pair?.kind !== 'tracking') return
    m.pushBranch.mutate(pair.branch.name, { onSuccess: () => closeModal() })
  }
  const runForcePush = () => m.pushForce.mutate(undefined, { onSuccess: () => closeModal() })
  const runRebaseOntoUpstream = () => {
    if (pair?.kind !== 'tracking') return
    m.rebase.mutate({ onto: pair.upstream }, { onSuccess: ({ result }) => onConflicts(result.conflicts) })
  }
  // Branch pair: bring source into target. Fast-forward and Blend both go
  // through merge-directional (it fast-forwards when it can); Stack rebases the
  // target branch onto the source.
  const runMergeIntoTarget = () => {
    if (!branchPair) return
    m.mergeDirectional.mutate(
      { target: branchPair.target.name, source: branchPair.source.name },
      { onSuccess: ({ result }) => onConflicts(result.conflicts) }
    )
  }
  const runRebaseTargetOntoSource = () => {
    if (!branchPair) return
    m.rebase.mutate(
      {
        onto: branchPair.source.name,
        branch: branchPair.target.name === headName ? undefined : branchPair.target.name,
      },
      { onSuccess: ({ result }) => onConflicts(result.conflicts) }
    )
  }

  return (
    <Dialog open={open} onOpenChange={(o) => !o && closeModal()}>
      <DialogContent className="gap-0 p-0 sm:max-w-md" aria-describedby={undefined}>
        <DialogHeader className="border-b border-border px-4 pb-3 pt-4">
          <DialogTitle className="flex items-center gap-2 text-sm">
            <Cloud size={15} strokeWidth={1.9} />
            Sync branch
          </DialogTitle>
        </DialogHeader>

        <div className="grid gap-3 px-4 py-4">
          <div className="flex items-center justify-center gap-3 rounded-md border border-border bg-panel2 px-3 py-3">
            <div className="flex flex-col items-center gap-1">
              <Chip name={flowFrom ?? ''} tone="source" />
              <span className="text-[9px] uppercase tracking-[.06em] text-muted-foreground">
                from
              </span>
            </div>
            <ArrowRight size={16} className="mt-[-14px] flex-none text-sub" />
            <div className="flex flex-col items-center gap-1">
              <Chip name={flowInto ?? ''} tone="target" />
              <span className="text-[9px] uppercase tracking-[.06em] text-muted-foreground">
                into
              </span>
            </div>
          </div>

          <div className="min-h-[34px] rounded-md border border-border bg-panel2 px-3 py-2 text-[11px]">
            {!pair && (
              <span className="flex items-center gap-1.5 text-muted-foreground">
                <AlertTriangle size={12} className="flex-none" />
                These two can't be synced together.
              </span>
            )}
            {branchPair && relation.isLoading && (
              <span className="text-muted-foreground">Checking how these compare…</span>
            )}
            {branchPair && relation.isError && (
              <span className="text-removed">{(relation.error as Error).message}</span>
            )}
            {action?.kind === 'up-to-date' && (
              <span className="flex items-center gap-1.5 text-muted-foreground">
                <Check size={12} className="flex-none" /> These already match. Nothing to do.
              </span>
            )}
            {pair?.kind === 'tracking' && action?.kind === 'fast-forward' && (
              <span className="flex items-center gap-1.5 text-added">
                <Zap size={12} className="flex-none" /> The cloud has {trackingSync.behind} newer
                change{trackingSync.behind === 1 ? '' : 's'}. Get {tgtName} up to date - clean and
                easy.
              </span>
            )}
            {pair?.kind === 'tracking' && action?.kind === 'push' && (
              <span className="flex items-center gap-1.5 text-primary">
                <Cloud size={12} className="flex-none" /> You have {trackingSync.ahead} change
                {trackingSync.ahead === 1 ? '' : 's'} the cloud doesn't. Send them up.
              </span>
            )}
            {action?.kind === 'upstream-gone' && (
              <span className="flex items-center gap-1.5 text-modified">
                <AlertTriangle size={12} className="flex-none" /> The cloud copy of this branch is
                gone. Send {tgtName} back up to recreate it.
              </span>
            )}
            {action?.kind === 'diverged-incoming' && (
              <span className="flex items-center gap-1.5 text-modified">
                <AlertTriangle size={12} className="flex-none" /> You both made changes. Stack yours
                on top of theirs (tidy), or blend them together.
              </span>
            )}
            {action?.kind === 'diverged-outgoing' && (
              <span className="flex items-center gap-1.5 text-modified">
                <AlertTriangle size={12} className="flex-none" /> Your history changed. Replace
                what's in the cloud with what you have now.
              </span>
            )}
            {action?.kind === 'same-point' && (
              <span className="flex items-center gap-1.5 text-muted-foreground">
                <Check size={12} className="flex-none" /> {tgtName} and {srcName} point at the same
                work. Nothing to do.
              </span>
            )}
            {action?.kind === 'contains' && (
              <span className="flex items-center gap-1.5 text-muted-foreground">
                <Check size={12} className="flex-none" /> {tgtName} already has everything from{' '}
                {srcName}. Nothing to do.
              </span>
            )}
            {action?.kind === 'ff-branch' && relation.data && (
              <span className="flex items-center gap-1.5 text-added">
                <Zap size={12} className="flex-none" /> {srcName} has {relation.data.behind} change
                {relation.data.behind === 1 ? '' : 's'} that {tgtName} doesn't. {tgtName} catches up
                - clean and easy.
              </span>
            )}
            {action?.kind === 'diverged-branches' && (
              <span className="flex items-center gap-1.5 text-modified">
                <AlertTriangle size={12} className="flex-none" /> Both branches have their own
                changes. Stack {tgtName}'s work on top of {srcName} (tidy), or blend them together.
              </span>
            )}
            {switchesBranch &&
              (action?.kind === 'ff-branch' || action?.kind === 'diverged-branches') && (
                <span className="mt-1 block text-[10px] text-muted-foreground">
                  This switches you to {tgtName} first.
                </span>
              )}
          </div>
        </div>

        <div className="flex justify-end gap-2 border-t border-border px-4 py-3">
          <Button variant="secondary" size="sm" onClick={closeModal}>
            Cancel
          </Button>

          {action?.kind === 'fast-forward' && (
            <Button size="sm" disabled={pending} onClick={runPull}>
              {pending ? 'Updating…' : 'Update'}
            </Button>
          )}
          {action?.kind === 'push' && (
            <Button size="sm" disabled={pending} onClick={runPush}>
              {pending ? 'Sending…' : 'Send up'}
            </Button>
          )}
          {action?.kind === 'upstream-gone' && (
            <Button size="sm" disabled={pending} onClick={runRepublish}>
              {pending ? 'Sending…' : 'Send up'}
            </Button>
          )}
          {action?.kind === 'diverged-incoming' && (
            <>
              <Button variant="secondary" size="sm" disabled={pending} onClick={runPull}>
                <GitMerge size={13} /> Blend
              </Button>
              <Button size="sm" disabled={pending} onClick={runRebaseOntoUpstream}>
                <Repeat2 size={13} /> {pending ? 'Stacking…' : 'Stack on top'}
              </Button>
            </>
          )}
          {action?.kind === 'diverged-outgoing' && (
            <Button size="sm" disabled={pending} onClick={runForcePush}>
              {pending ? 'Replacing…' : 'Replace cloud copy'}
            </Button>
          )}
          {action?.kind === 'ff-branch' && (
            <Button size="sm" disabled={pending} onClick={runMergeIntoTarget}>
              {pending ? 'Updating…' : `Update ${tgtName}`}
            </Button>
          )}
          {action?.kind === 'diverged-branches' && (
            <>
              <Button variant="secondary" size="sm" disabled={pending} onClick={runMergeIntoTarget}>
                <GitMerge size={13} /> Blend
              </Button>
              <Button size="sm" disabled={pending} onClick={runRebaseTargetOntoSource}>
                <Repeat2 size={13} /> {pending ? 'Stacking…' : 'Stack on top'}
              </Button>
            </>
          )}
        </div>
      </DialogContent>
    </Dialog>
  )
}
