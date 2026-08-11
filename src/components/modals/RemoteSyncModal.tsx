import { useEffect, useMemo, useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import {
  AlertTriangle,
  ArrowDown,
  ArrowRight,
  ArrowUp,
  Check,
  Cloud,
  CloudUpload,
  GitMerge,
  Info,
  Repeat2,
  RotateCcw,
  ArrowUpDown,
} from 'lucide-react'
import { cn } from '@/lib/utils'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { TooltipButton } from '@/components/ui/tooltip'
import { commands } from '@/lib/bindings'
import { unwrap } from '@/lib/queryKeys'
import { resolveDropPair, type DropPair } from '@/lib/refSync'
import { useBranches } from '@/hooks/useGitQueries'
import { branchSync } from '@/lib/branchActions'
import { useGitMutations } from '@/hooks/useGitMutations'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo } from '@/stores/workspaceStore'
import { modeCopy, modesFor, type Divergence, type PreviewMode, type Tone } from '@/lib/syncPreview'
import { SyncTreePreview } from './SyncTreePreview'

/** Icon per mode, used on both the mode button and the confirm button. */
const MODE_ICON: Record<PreviewMode, typeof GitMerge> = {
  replace: CloudUpload,
  blend: GitMerge,
  stack: Repeat2,
  reset: RotateCcw,
  get: ArrowDown,
  send: ArrowUp,
}

/** A ref chip styled to match the graph's pills. */
function Chip({ name, tone }: { name: string; tone: 'source' | 'target' }) {
  return (
    <span
      className={cn(
        'block max-w-[150px] overflow-hidden text-ellipsis whitespace-nowrap rounded-[5px] border px-2 py-1 text-center font-mono text-2xs font-semibold',
        tone === 'target'
          ? 'border-primary bg-soft text-accent-text'
          : 'border-border bg-panel2 text-foreground'
      )}
    >
      {name || '…'}
    </span>
  )
}

const TONE_TEXT: Record<Tone, string> = {
  good: 'text-accent-text',
  warn: 'text-modified',
  bad: 'text-removed',
  plain: 'text-sub',
}
const TONE_BOX: Record<Tone, string> = {
  good: 'border-primary/25 bg-soft',
  warn: 'border-modified/25 bg-modified/10',
  bad: 'border-removed/25 bg-removed/10',
  plain: 'border-border bg-panel2',
}

export function RemoteSyncModal() {
  const open = useUiStore((s) => s.activeModal === 'remote-sync')
  const closeModal = useUiStore((s) => s.closeModal)
  const openConflict = useUiStore((s) => s.openConflict)
  const resetToBranchPrompt = useUiStore((s) => s.resetToBranchPrompt)
  const swapSync = useUiStore((s) => s.swapSync)
  const syncSource = useUiStore((s) => s.syncSource)
  const syncTarget = useUiStore((s) => s.syncTarget)
  const preselectedMode = useUiStore((s) => s.syncMode)

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

  // The one number pair everything below is derived from: commits only we have,
  // commits only they have. A tracking pair reads it from `sync` (the raw
  // ahead/behind on BranchInfo report 0/0 for non-diverged states); a branch
  // pair gets it from branchRelation.
  const divergence: Divergence | null = useMemo(() => {
    if (pair?.kind === 'tracking') {
      const s = branchSync(pair.branch)
      if (pair.branch.sync.kind === 'upstream_gone') return null
      return { ours: s.ahead, theirs: s.behind }
    }
    if (branchPair && relation.data) {
      return { ours: relation.data.ahead, theirs: relation.data.behind }
    }
    return null
  }, [pair, branchPair, relation.data])

  const upstreamGone = pair?.kind === 'tracking' && pair.branch.sync.kind === 'upstream_gone'
  const modes = divergence ? modesFor(divergence) : []

  // Which option is selected. Resets whenever the pair changes, so a fresh drag
  // never inherits a destructive selection from the previous one -- but a mode
  // the caller asked for (picking "Stack on top" from a menu) is honoured, so
  // the menu item and the window agree on what was chosen.
  const [mode, setMode] = useState<PreviewMode | null>(null)
  useEffect(() => {
    setMode(preselectedMode)
  }, [syncSource, syncTarget, preselectedMode])
  // A preselect that doesn't apply to how these two actually diverged falls
  // back to the default, so a stale hint can never run something unintended.
  const active: PreviewMode | null = mode && modes.includes(mode) ? mode : (modes.at(-1) ?? null)

  // Names. For a tracking pair "ours" is the local branch; for a branch pair it
  // is the branch that moves (the target), which is the one the copy is about.
  const ourName = pair?.kind === 'tracking' ? pair.branch.name : (branchPair?.target.name ?? '')
  const theirName = pair?.kind === 'tracking' ? pair.upstream : (branchPair?.source.name ?? '')

  // Reset only re-points HEAD's branch, so it is offered only when the branch
  // that would move IS the checked-out one.
  const headName = branches.data?.local.find((b) => b.is_head)?.name
  const canReset = !!branchPair && branchPair.target.name === headName
  const switchesBranch = !!branchPair && branchPair.target.name !== headName

  // Swapping is only meaningful when flipping still resolves to a valid pair:
  // both refs must be local branches, so either can be the one that moves.
  const canSwap =
    !!branchPair &&
    !!branches.data?.local.some((b) => b.name === syncSource) &&
    !!branches.data?.local.some((b) => b.name === syncTarget)

  const pending =
    m.pull.isPending ||
    m.push.isPending ||
    m.pushBranch.isPending ||
    m.pushForce.isPending ||
    m.rebase.isPending ||
    m.mergeDirectional.isPending ||
    m.fastForwardBranch.isPending

  const onConflicts = (conflicts: string[]) => {
    closeModal()
    if (conflicts.length > 0) openConflict(conflicts[0])
  }
  const done = { onSuccess: () => closeModal() }

  /** Map the chosen mode onto the mutation that performs it. */
  const run = () => {
    if (!active) return
    const tracking = pair?.kind === 'tracking' ? pair : null

    switch (active) {
      case 'get':
        if (tracking) return void m.pull.mutate(undefined, done)
        if (branchPair)
          return void m.fastForwardBranch.mutate(
            { branch: branchPair.target.name, target: branchPair.source.name },
            done
          )
        return
      case 'send':
        if (tracking) return void m.push.mutate(undefined, done)
        return
      case 'replace':
        return void m.pushForce.mutate(undefined, done)
      case 'blend':
        if (tracking) return void m.pull.mutate(undefined, done)
        if (branchPair)
          return void m.mergeDirectional.mutate(
            { target: branchPair.target.name, source: branchPair.source.name },
            { onSuccess: ({ result }) => onConflicts(result.conflicts) }
          )
        return
      case 'stack':
        if (tracking)
          return void m.rebase.mutate(
            { onto: tracking.upstream },
            { onSuccess: ({ result }) => onConflicts(result.conflicts) }
          )
        if (branchPair)
          return void m.rebase.mutate(
            {
              onto: branchPair.source.name,
              branch: branchPair.target.name === headName ? undefined : branchPair.target.name,
            },
            { onSuccess: ({ result }) => onConflicts(result.conflicts) }
          )
        return
      case 'reset':
        if (!branchPair) return
        closeModal()
        return void resetToBranchPrompt(branchPair.source.name)
    }
  }

  // Republishing a branch whose cloud copy was deleted. Not a divergence, so it
  // sits outside the mode machinery entirely.
  const runRepublish = () => {
    if (pair?.kind !== 'tracking') return
    m.pushBranch.mutate(pair.branch.name, done)
  }

  const copy = active && divergence ? modeCopy(active, divergence) : null
  // Reset is an extra option on branch pairs, not one of the three the
  // divergence implies, so it is appended rather than returned by modesFor.
  const shown: PreviewMode[] = canReset && modes.length === 3 ? [...modes, 'reset'] : modes

  return (
    <Dialog open={open} onOpenChange={(o) => !o && closeModal()}>
      <DialogContent className="gap-0 overflow-hidden p-0 sm:max-w-md" aria-describedby={undefined}>
        <DialogHeader className="border-b border-border px-4 pb-3 pt-4">
          <DialogTitle className="flex items-center gap-2 text-sm">
            <Cloud size={15} strokeWidth={1.9} />
            Sync branch
          </DialogTitle>
        </DialogHeader>

        <div className="grid min-w-0 gap-2.5 px-4 py-3.5">
          {/* Direction. The chips say where commits end up, and Swap fixes a
              drag that went the wrong way without re-dragging. */}
          <div className="flex min-w-0 items-center gap-2.5 rounded-md border border-border bg-panel2 px-2.5 py-2">
            <div className="min-w-0">
              <Chip name={ourName || syncSource || ''} tone="source" />
              <span className="mt-0.5 block text-center text-2xs uppercase tracking-[.06em] text-muted-foreground">
                from
              </span>
            </div>
            <ArrowRight size={15} className="-mt-3 flex-none text-sub" />
            <div className="min-w-0">
              <Chip name={theirName || syncTarget || ''} tone="target" />
              <span className="mt-0.5 block text-center text-2xs uppercase tracking-[.06em] text-muted-foreground">
                into
              </span>
            </div>
            {canSwap && (
              <TooltipButton
                onClick={swapSync}
                tooltip="Swap direction"
                className="-mt-3 ml-auto flex size-7 flex-none items-center justify-center rounded border border-border bg-panel text-sub hover:border-primary hover:text-accent-text"
              >
                <ArrowUpDown size={13} />
              </TooltipButton>
            )}
          </div>

          {!pair && (
            <div className="flex items-center gap-1.5 rounded-md border border-border bg-panel2 px-3 py-2 text-2xs text-muted-foreground">
              <AlertTriangle size={12} className="flex-none" />
              These two can't be synced together.
            </div>
          )}

          {branchPair && relation.isLoading && (
            <div className="rounded-md border border-border bg-panel2 px-3 py-2 text-2xs text-muted-foreground">
              Checking how these compare…
            </div>
          )}
          {branchPair && relation.isError && (
            <div className="rounded-md border border-border bg-panel2 px-3 py-2 text-2xs text-removed">
              {(relation.error as Error).message}
            </div>
          )}

          {upstreamGone && (
            <div className="flex items-start gap-1.5 rounded-md border border-modified/25 bg-modified/10 px-3 py-2 text-2xs text-modified">
              <AlertTriangle size={12} className="mt-px flex-none" />
              <span>The cloud copy of this branch is gone. Send {ourName} back up to recreate it.</span>
            </div>
          )}

          {divergence && modes.length === 0 && (
            <div className="flex items-center gap-1.5 rounded-md border border-border bg-panel2 px-3 py-2 text-2xs text-muted-foreground">
              <Check size={12} className="flex-none" />
              These already match. Nothing to do.
            </div>
          )}

          {/* Mode buttons: label, icon, and the consequence underneath. */}
          {divergence && shown.length > 0 && (
            <div
              className="grid min-w-0 gap-1.5"
              style={{ gridTemplateColumns: `repeat(${shown.length}, minmax(0, 1fr))` }}
            >
              {shown.map((k) => {
                const c = modeCopy(k, divergence)
                const Icon = MODE_ICON[k]
                const on = k === active
                return (
                  <button
                    key={k}
                    type="button"
                    onClick={() => setMode(k)}
                    className={cn(
                      'min-w-0 rounded-md border px-2 py-1.5 text-left transition-colors',
                      on && c.danger && 'border-removed bg-removed/10',
                      on && !c.danger && 'border-primary bg-soft',
                      !on && 'border-border bg-panel2 hover:border-border/80 hover:bg-panel3'
                    )}
                  >
                    <span
                      className={cn(
                        'flex min-w-0 items-center gap-1.5 text-2xs font-semibold',
                        on && c.danger && 'text-removed',
                        on && !c.danger && 'text-accent-text'
                      )}
                    >
                      <Icon size={11} className="flex-none" />
                      <span className="overflow-hidden text-ellipsis whitespace-nowrap">{c.label}</span>
                    </span>
                    <span
                      className={cn(
                        'mt-0.5 block overflow-hidden text-ellipsis whitespace-nowrap text-2xs',
                        on && c.danger && 'text-removed/80',
                        on && !c.danger && 'text-accent-text/75',
                        !on && 'text-muted-foreground'
                      )}
                    >
                      {c.sub}
                    </span>
                  </button>
                )
              })}
            </div>
          )}

          {/* The picture: current shape on the left, result on the right. */}
          {divergence && active && copy && (
            <>
              <div className="min-w-0 overflow-hidden rounded-md border border-border bg-panel2 p-2.5">
                <SyncTreePreview divergence={divergence} mode={active} />
                <div className="mt-2 flex min-w-0 items-center justify-between gap-2 border-t border-border pt-2">
                  <span className="min-w-0 overflow-hidden text-ellipsis whitespace-nowrap text-2xs text-sub">
                    {copy.caption}
                  </span>
                  <span
                    className={cn(
                      'flex-none rounded-full border px-2 py-0.5 text-2xs',
                      copy.pill.tone === 'bad'
                        ? 'border-removed/30 bg-removed/10 text-removed'
                        : 'border-primary/30 bg-soft text-accent-text'
                    )}
                  >
                    {copy.pill.text}
                  </span>
                </div>
              </div>

              <div
                className={cn(
                  'flex min-w-0 items-start gap-1.5 rounded-md border px-2.5 py-2 text-2xs leading-relaxed',
                  TONE_BOX[copy.note.tone],
                  TONE_TEXT[copy.note.tone]
                )}
              >
                {copy.note.tone === 'good' ? (
                  <Check size={12} className="mt-px flex-none" />
                ) : copy.note.tone === 'plain' ? (
                  <Info size={12} className="mt-px flex-none" />
                ) : (
                  <AlertTriangle size={12} className="mt-px flex-none" />
                )}
                <span className="min-w-0">{copy.note.text}</span>
              </div>

              {switchesBranch && (
                <span className="text-2xs text-muted-foreground">
                  This switches you to {ourName} first.
                </span>
              )}
            </>
          )}
        </div>

        <div className="flex justify-end gap-2 border-t border-border px-4 py-3">
          <Button variant="secondary" size="sm" onClick={closeModal}>
            Cancel
          </Button>
          {upstreamGone && (
            <Button size="sm" disabled={pending} onClick={runRepublish}>
              {pending ? 'Sending…' : 'Send up'}
            </Button>
          )}
          {copy && active && (
            <Button
              size="sm"
              disabled={pending}
              onClick={run}
              className={cn(copy.danger && 'bg-removed text-background hover:bg-removed/90')}
            >
              {(() => {
                const Icon = MODE_ICON[active]
                return <Icon size={13} />
              })()}
              {pending ? 'Working…' : copy.action}
            </Button>
          )}
        </div>
      </DialogContent>
    </Dialog>
  )
}
