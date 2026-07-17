import { useMemo } from 'react'
import { AlertTriangle, ArrowRight, Check, Cloud, GitMerge, Repeat2, Zap } from 'lucide-react'
import { cn } from '@/lib/utils'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { resolveSyncPair, type RefSyncPair } from '@/lib/refSync'
import { useBranches } from '@/hooks/useGitQueries'
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
 * The concrete action a drop resolves to. The branch's actual ahead/behind
 * state decides - not which way the pills were dragged - so any drag between a
 * branch and its remote offers whatever would put them in sync. Drag direction
 * only picks the options for the diverged case (dropping local onto remote
 * reads as "make the cloud look like me" -> offer the overwrite).
 */
type Action =
  | { kind: 'up-to-date' }
  | { kind: 'fast-forward' }
  | { kind: 'push' }
  | { kind: 'diverged-incoming' }
  | { kind: 'diverged-outgoing' }

function chooseAction(pair: RefSyncPair): Action {
  const { ahead, behind } = pair.branch
  if (ahead === 0 && behind === 0) return { kind: 'up-to-date' }
  if (behind > 0 && ahead === 0) return { kind: 'fast-forward' }
  if (ahead > 0 && behind === 0) return { kind: 'push' }
  return pair.direction === 'outgoing'
    ? { kind: 'diverged-outgoing' }
    : { kind: 'diverged-incoming' }
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

  const pair = useMemo<RefSyncPair | null>(() => {
    if (!syncSource || !syncTarget || !branches.data) return null
    // A name matching a local branch is local (even if it contains a slash,
    // like `feature/foo`); otherwise it's the remote-tracking ref.
    const t = (name: string): 'remote' | 'head' | 'branch' => {
      const local = branches.data!.local.find((b) => b.name === name)
      if (!local) return 'remote'
      return local.is_head ? 'head' : 'branch'
    }
    return resolveSyncPair(
      { name: syncSource, type: t(syncSource) },
      { name: syncTarget, type: t(syncTarget) },
      branches.data
    )
  }, [syncSource, syncTarget, branches.data])

  const action = pair ? chooseAction(pair) : null

  // The chips show where commits will actually flow, which may be the reverse
  // of the drag: dropping origin/main onto main while you're ahead means a
  // push, so the picture reads main -> origin/main. Falls back to drag order
  // when the pair is invalid.
  const commitsGoUp = action?.kind === 'push' || action?.kind === 'diverged-outgoing'
  const flowFrom = pair ? (commitsGoUp ? pair.branch.name : pair.upstream) : syncSource
  const flowInto = pair ? (commitsGoUp ? pair.upstream : pair.branch.name) : syncTarget

  const pending =
    m.pull.isPending || m.push.isPending || m.pushForce.isPending || m.rebase.isPending

  const onConflicts = (conflicts: string[]) => {
    closeModal()
    if (conflicts.length > 0) openConflict(conflicts[0])
  }

  const runPull = () =>
    m.pull.mutate(undefined, { onSuccess: () => closeModal(), onError: () => {} })
  const runPush = () => m.push.mutate(undefined, { onSuccess: () => closeModal() })
  const runForcePush = () => m.pushForce.mutate(undefined, { onSuccess: () => closeModal() })
  const runRebase = () => {
    if (!pair) return
    m.rebase.mutate(pair.upstream, { onSuccess: ({ result }) => onConflicts(result.conflicts) })
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
                These two aren't linked, so there's nothing to sync between them.
              </span>
            )}
            {pair && action?.kind === 'up-to-date' && (
              <span className="flex items-center gap-1.5 text-muted-foreground">
                <Check size={12} className="flex-none" /> These already match. Nothing to do.
              </span>
            )}
            {pair && action?.kind === 'fast-forward' && (
              <span className="flex items-center gap-1.5 text-added">
                <Zap size={12} className="flex-none" /> The cloud has {pair.branch.behind} newer
                change{pair.branch.behind === 1 ? '' : 's'}. Get {pair.branch.name} up to date - clean
                and easy.
              </span>
            )}
            {pair && action?.kind === 'push' && (
              <span className="flex items-center gap-1.5 text-primary">
                <Cloud size={12} className="flex-none" /> You have {pair.branch.ahead} change
                {pair.branch.ahead === 1 ? '' : 's'} the cloud doesn't. Send them up.
              </span>
            )}
            {pair && action?.kind === 'diverged-incoming' && (
              <span className="flex items-center gap-1.5 text-modified">
                <AlertTriangle size={12} className="flex-none" /> You both made changes. Stack yours on
                top of theirs (tidy), or blend them together.
              </span>
            )}
            {pair && action?.kind === 'diverged-outgoing' && (
              <span className="flex items-center gap-1.5 text-modified">
                <AlertTriangle size={12} className="flex-none" /> Your history changed. Replace what's
                in the cloud with what you have now.
              </span>
            )}
          </div>
        </div>

        <div className="flex justify-end gap-2 border-t border-border px-4 py-3">
          <Button variant="secondary" size="sm" onClick={closeModal}>
            Cancel
          </Button>

          {pair && action?.kind === 'fast-forward' && (
            <Button size="sm" disabled={pending} onClick={runPull}>
              {pending ? 'Updating…' : 'Update'}
            </Button>
          )}
          {pair && action?.kind === 'push' && (
            <Button size="sm" disabled={pending} onClick={runPush}>
              {pending ? 'Sending…' : 'Send up'}
            </Button>
          )}
          {pair && action?.kind === 'diverged-incoming' && (
            <>
              <Button variant="secondary" size="sm" disabled={pending} onClick={runPull}>
                <GitMerge size={13} /> Blend
              </Button>
              <Button size="sm" disabled={pending} onClick={runRebase}>
                <Repeat2 size={13} /> {pending ? 'Stacking…' : 'Stack on top'}
              </Button>
            </>
          )}
          {pair && action?.kind === 'diverged-outgoing' && (
            <Button size="sm" disabled={pending} onClick={runForcePush}>
              {pending ? 'Replacing…' : 'Replace cloud copy'}
            </Button>
          )}
        </div>
      </DialogContent>
    </Dialog>
  )
}
