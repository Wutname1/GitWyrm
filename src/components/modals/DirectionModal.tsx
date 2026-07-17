import { useEffect, useMemo, useState } from 'react'
import { AlertTriangle, ArrowRight, GitMerge, Repeat, Zap } from 'lucide-react'
import { cn } from '@/lib/utils'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { commands, type MergeAnalysis } from '@/lib/bindings'
import { unwrap } from '@/lib/queryKeys'
import { useBranches } from '@/hooks/useGitQueries'
import { useGitMutations } from '@/hooks/useGitMutations'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo } from '@/stores/workspaceStore'

/** A branch chip styled to match the graph's branch RefBadge. */
function Chip({ name, tone }: { name: string; tone: 'source' | 'target' }) {
  return (
    <span
      className={cn(
        'max-w-[130px] overflow-hidden text-ellipsis whitespace-nowrap rounded-[5px] border px-2 py-1 font-mono text-[11px] font-semibold',
        tone === 'target'
          ? 'border-primary bg-soft text-primary'
          : 'border-border bg-panel2 text-foreground'
      )}
    >
      {name || '…'}
    </span>
  )
}

export function DirectionModal() {
  const open = useUiStore((s) => s.activeModal === 'merge')
  const closeModal = useUiStore((s) => s.closeModal)
  const openConflict = useUiStore((s) => s.openConflict)
  const preselected = useUiStore((s) => s.mergeSource)

  const repo = useActiveRepo()
  const branches = useBranches(repo?.id ?? null)
  const m = useGitMutations(repo?.id ?? null)

  const current = branches.data?.local.find((b) => b.is_head)?.name ?? ''
  const candidates = useMemo(
    () => [
      ...(branches.data?.local ?? []).filter((b) => !b.is_head).map((b) => b.name),
      ...(branches.data?.remote ?? []),
    ],
    [branches.data]
  )

  // The "other" branch (the one that isn't current). `reversed` flips which of
  // {other, current} is the merge source vs. the target that receives commits.
  const [other, setOther] = useState<string | null>(null)
  const [reversed, setReversed] = useState(false)
  const [analysis, setAnalysis] = useState<MergeAnalysis | null>(null)
  const [analyzing, setAnalyzing] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    if (!open) return
    setOther(preselected ?? null)
    setReversed(false)
    setAnalysis(null)
    setError(null)
  }, [open, preselected])

  // Forward (default): merge `other` into `current`. Reversed: merge `current`
  // into `other`, which requires switching to `other` first.
  const source = reversed ? current : (other ?? '')
  const target = reversed ? (other ?? '') : current
  const switchesBranch = reversed

  // Preview the merge outcome for the current direction. Analysis is always run
  // as "merge source into target"; when reversed we can't preview without
  // switching branches, so we skip the live analysis and warn instead.
  useEffect(() => {
    if (!open || !repo || !other || reversed) {
      setAnalysis(null)
      return
    }
    let cancelled = false
    setAnalyzing(true)
    setError(null)
    void (async () => {
      try {
        const res = unwrap(await commands.mergeAnalysis(repo.id, other))
        if (!cancelled) setAnalysis(res)
      } catch (e) {
        if (!cancelled) setError((e as Error).message)
      } finally {
        if (!cancelled) setAnalyzing(false)
      }
    })()
    return () => {
      cancelled = true
    }
  }, [open, repo, other, reversed])

  const pending = m.merge.isPending || m.mergeDirectional.isPending
  const canRun =
    !!other && !pending && !(analysis?.up_to_date === true && !reversed)

  const doMerge = () => {
    if (!other) return
    const onDone = (result: { conflicts: string[] }) => {
      closeModal()
      if (result.conflicts.length > 0) openConflict(result.conflicts[0])
    }
    if (reversed) {
      m.mergeDirectional.mutate(
        { target, source },
        { onSuccess: ({ result }) => onDone(result) }
      )
    } else {
      m.merge.mutate(other, { onSuccess: ({ result }) => onDone(result) })
    }
  }

  return (
    <Dialog open={open} onOpenChange={(o) => !o && closeModal()}>
      <DialogContent className="gap-0 p-0 sm:max-w-md" aria-describedby={undefined}>
        <DialogHeader className="border-b border-border px-4 pb-3 pt-4">
          <DialogTitle className="flex items-center gap-2 text-sm">
            <GitMerge size={15} strokeWidth={1.9} />
            Merge branches
          </DialogTitle>
        </DialogHeader>

        <div className="grid gap-3 px-4 py-4">
          <div className="flex items-center justify-center gap-3 rounded-md border border-border bg-panel2 px-3 py-3">
            <div className="flex flex-col items-center gap-1">
              <Chip name={source} tone="source" />
              <span className="text-[9px] uppercase tracking-[.06em] text-muted-foreground">
                bring work from
              </span>
            </div>
            <ArrowRight size={16} className="mt-[-14px] flex-none text-sub" />
            <div className="flex flex-col items-center gap-1">
              <Chip name={target} tone="target" />
              <span className="text-[9px] uppercase tracking-[.06em] text-muted-foreground">
                into
              </span>
            </div>
            <button
              onClick={() => setReversed((r) => !r)}
              disabled={!other}
              title="Swap direction"
              className="ml-1 flex size-7 flex-none items-center justify-center rounded border border-border bg-panel text-sub hover:border-primary hover:text-primary disabled:cursor-not-allowed disabled:opacity-40"
            >
              <Repeat size={13} />
            </button>
          </div>

          <div className="grid gap-1.5">
            <label className="text-[11px] font-semibold text-sub">Other branch</label>
            <div className="max-h-[180px] overflow-y-auto rounded-md border border-border bg-background p-1">
              {candidates.length === 0 && (
                <div className="px-2 py-1.5 text-[11px] text-muted-foreground">
                  No other branches to merge.
                </div>
              )}
              {candidates.map((name) => (
                <button
                  key={name}
                  onClick={() => setOther(name)}
                  className={cn(
                    'flex w-full items-center gap-2 rounded px-2 py-1 text-left font-mono text-[11.5px] hover:bg-panel3',
                    other === name ? 'bg-soft text-primary' : 'text-foreground'
                  )}
                >
                  <span
                    className={cn(
                      'size-1.5 flex-none rounded-full',
                      other === name ? 'bg-primary' : 'bg-muted-foreground'
                    )}
                  />
                  {name}
                </button>
              ))}
            </div>
          </div>

          <div className="min-h-[34px] rounded-md border border-border bg-panel2 px-3 py-2 text-[11px]">
            {!other && (
              <span className="text-muted-foreground">Pick a branch to preview the merge.</span>
            )}
            {other && switchesBranch && (
              <span className="flex items-center gap-1.5 text-modified">
                <AlertTriangle size={12} className="flex-none" />
                Switches to {target} first, then merges {source} in.
              </span>
            )}
            {other && !switchesBranch && analyzing && (
              <span className="text-muted-foreground">Analyzing…</span>
            )}
            {other && !switchesBranch && error && <span className="text-removed">{error}</span>}
            {other && !switchesBranch && analysis && !analyzing && !error && (
              <>
                {analysis.up_to_date && (
                  <span className="text-muted-foreground">
                    {target} already contains {source}. Nothing to merge.
                  </span>
                )}
                {analysis.can_fast_forward && (
                  <span className="flex items-center gap-1.5 text-added">
                    <Zap size={12} /> Clean update — {target} just catches up to {source}.
                  </span>
                )}
                {analysis.normal && !analysis.can_fast_forward && (
                  <span className="flex items-center gap-1.5 text-modified">
                    <AlertTriangle size={12} /> Combines both branches. You may need to fix a few
                    overlaps.
                  </span>
                )}
              </>
            )}
          </div>
        </div>

        <div className="flex justify-end gap-2 border-t border-border px-4 py-3">
          <Button variant="secondary" size="sm" onClick={closeModal}>
            Cancel
          </Button>
          <Button size="sm" disabled={!canRun} onClick={doMerge}>
            {pending ? 'Merging…' : 'Merge'}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  )
}
