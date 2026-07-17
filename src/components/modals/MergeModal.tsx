import { useEffect, useMemo, useState } from 'react'
import { AlertTriangle, GitMerge, Zap } from 'lucide-react'
import { cn } from '@/lib/utils'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { commands, type MergeAnalysis } from '@/lib/bindings'
import { unwrap } from '@/lib/queryKeys'
import { useBranches } from '@/hooks/useGitQueries'
import { useGitMutations } from '@/hooks/useGitMutations'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo } from '@/stores/workspaceStore'

export function MergeModal() {
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

  const [source, setSource] = useState<string | null>(null)
  const [analysis, setAnalysis] = useState<MergeAnalysis | null>(null)
  const [analyzing, setAnalyzing] = useState(false)
  const [error, setError] = useState<string | null>(null)

  // Reset selection each time the modal opens, honoring any preselected branch.
  useEffect(() => {
    if (!open) return
    setSource(preselected ?? null)
    setAnalysis(null)
    setError(null)
  }, [open, preselected])

  // Preview the merge outcome whenever the source changes.
  useEffect(() => {
    if (!open || !repo || !source) {
      setAnalysis(null)
      return
    }
    let cancelled = false
    setAnalyzing(true)
    setError(null)
    void (async () => {
      try {
        const res = unwrap(await commands.mergeAnalysis(repo.id, source))
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
  }, [open, repo, source])

  const doMerge = () => {
    if (!source) return
    m.merge.mutate(source, {
      onSuccess: ({ result }) => {
        closeModal()
        if (result.conflicts.length > 0) openConflict(result.conflicts[0])
      },
    })
  }

  return (
    <Dialog open={open} onOpenChange={(o) => !o && closeModal()}>
      <DialogContent className="gap-0 p-0 sm:max-w-md" aria-describedby={undefined}>
        <DialogHeader className="border-b border-border px-4 pb-3 pt-4">
          <DialogTitle className="flex items-center gap-2 text-sm">
            <GitMerge size={15} strokeWidth={1.9} />
            Merge a branch
          </DialogTitle>
        </DialogHeader>

        <div className="grid gap-3 px-4 py-4">
          <div className="flex items-center gap-2 text-xs text-sub">
            <span>Merge</span>
            <span className="font-mono font-semibold text-primary">{source ?? '…'}</span>
            <span>into</span>
            <span className="rounded bg-soft px-1.5 py-0.5 font-mono font-semibold text-foreground">
              {current || 'HEAD'}
            </span>
          </div>

          <div className="grid gap-1.5">
            <label className="text-[11px] font-semibold text-sub">Source branch</label>
            <div className="max-h-[200px] overflow-y-auto rounded-md border border-border bg-background p-1">
              {candidates.length === 0 && (
                <div className="px-2 py-1.5 text-[11px] text-muted-foreground">
                  No other branches to merge.
                </div>
              )}
              {candidates.map((name) => (
                <button
                  key={name}
                  onClick={() => setSource(name)}
                  className={cn(
                    'flex w-full items-center gap-2 rounded px-2 py-1 text-left font-mono text-[11.5px] hover:bg-panel3',
                    source === name ? 'bg-soft text-primary' : 'text-foreground'
                  )}
                >
                  <span
                    className={cn(
                      'size-1.5 flex-none rounded-full',
                      source === name ? 'bg-primary' : 'bg-muted-foreground'
                    )}
                  />
                  {name}
                </button>
              ))}
            </div>
          </div>

          <div className="min-h-[34px] rounded-md border border-border bg-panel2 px-3 py-2 text-[11px]">
            {!source && <span className="text-muted-foreground">Pick a branch to preview the merge.</span>}
            {source && analyzing && <span className="text-muted-foreground">Analyzing…</span>}
            {source && error && <span className="text-removed">{error}</span>}
            {source && analysis && !analyzing && !error && (
              <>
                {analysis.up_to_date && (
                  <span className="text-muted-foreground">
                    {current || 'HEAD'} already contains {source}. Nothing to merge.
                  </span>
                )}
                {analysis.can_fast_forward && (
                  <span className="flex items-center gap-1.5 text-added">
                    <Zap size={12} /> Fast-forward to{' '}
                    <span className="font-mono">{analysis.target_sha}</span> — no merge commit.
                  </span>
                )}
                {analysis.normal && !analysis.can_fast_forward && (
                  <span className="flex items-center gap-1.5 text-modified">
                    <AlertTriangle size={12} /> Creates a merge commit. Conflicts may need
                    resolving.
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
          <Button
            size="sm"
            disabled={!source || m.merge.isPending || analysis?.up_to_date === true}
            onClick={doMerge}
          >
            {m.merge.isPending ? 'Merging…' : 'Merge'}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  )
}
