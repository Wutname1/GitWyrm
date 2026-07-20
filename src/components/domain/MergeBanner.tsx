import { AlertTriangle, GitMerge, X } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { PendingIndicator } from '@/components/ui/pending-indicator'
import { TooltipButton } from '@/components/ui/tooltip'
import { useMergeState } from '@/hooks/useGitQueries'
import { useGitMutations } from '@/hooks/useGitMutations'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo } from '@/stores/workspaceStore'
import type { OperationKind } from '@/lib/bindings'

/** Cleans up the state message's first line into just the branch/commit label. */
function incomingName(label: string | null | undefined): string {
  if (!label) return 'incoming changes'
  const match = label.match(/Merge (?:branch|remote-tracking branch) '([^']+)'/)
  return match ? match[1] : label
}

const COPY: Record<OperationKind, { verb: string; finish: string; abort: string }> = {
  Merge: { verb: 'Merging', finish: 'Commit merge', abort: 'Abort merge' },
  CherryPick: { verb: 'Cherry-picking', finish: 'Commit pick', abort: 'Abort cherry-pick' },
  Revert: { verb: 'Reverting', finish: 'Commit revert', abort: 'Abort revert' },
  Rebase: { verb: 'Rebasing', finish: 'Continue rebase', abort: 'Abort rebase' },
}

export function MergeBanner() {
  const repo = useActiveRepo()
  const merge = useMergeState(repo?.id ?? null)
  const m = useGitMutations(repo?.id ?? null)
  const openConflict = useUiStore((s) => s.openConflict)

  const state = merge.data
  if (!state?.merging || !state.operation) return null

  const isRebase = state.operation === 'Rebase'
  const copy = COPY[state.operation]

  const conflicts = state.conflicts
  const remaining = conflicts.length
  const label = incomingName(state.incoming_label)

  const finishPending = m.commitMerge.isPending || m.rebaseContinue.isPending
  const abortPending = m.abortMerge.isPending || m.rebaseAbort.isPending

  const finish = () => {
    if (isRebase) {
      m.rebaseContinue.mutate()
      return
    }
    // The full prepared message (MERGE_MSG) keeps multi-line cherry-pick and
    // merge messages intact; fall back to a built one if it's missing.
    const fallback = state.operation === 'Merge' ? `Merge ${label}` : label
    m.commitMerge.mutate(state.full_message?.trim() || fallback)
  }

  const abort = () => {
    if (isRebase) m.rebaseAbort.mutate()
    else m.abortMerge.mutate()
  }

  return (
    <div className="flex flex-none items-center gap-3 border-b border-modified/40 bg-modified/[.08] px-3.5 py-2">
      <GitMerge size={15} className="flex-none text-modified" strokeWidth={2} />
      <div className="min-w-0 flex-1">
        <div className="text-[11.5px] font-semibold text-foreground">
          {copy.verb} <span className="font-mono text-modified">{label}</span>
        </div>
        <div className="flex items-center gap-1.5 text-[10.5px] text-sub">
          {remaining > 0 ? (
            <>
              <AlertTriangle size={11} className="text-removed" />
              {remaining} conflict{remaining === 1 ? '' : 's'} left to resolve
            </>
          ) : isRebase ? (
            'All conflicts resolved — ready to continue.'
          ) : (
            'All conflicts resolved — ready to commit.'
          )}
        </div>
      </div>

      {remaining > 0 && (
        <button
          onClick={() => openConflict(conflicts[0])}
          className="flex-none rounded border border-removed/50 bg-removed/10 px-2.5 py-1 text-[11px] font-semibold text-removed hover:bg-removed/20"
        >
          Resolve conflicts
        </button>
      )}

      <Button
        size="sm"
        disabled={remaining > 0 || finishPending || abortPending}
        onClick={finish}
        className="h-7 flex-none text-[11px]"
      >
        {finishPending ? (isRebase ? 'Continuing…' : 'Committing…') : copy.finish}
      </Button>

      <TooltipButton
        onClick={abort}
        tooltip={copy.abort}
        disabled={abortPending || finishPending}
        aria-busy={abortPending || undefined}
        className="flex h-7 flex-none items-center justify-center gap-1.5 rounded border border-border bg-panel2 px-2 text-[11px] text-sub hover:border-muted-foreground hover:bg-panel3 disabled:pointer-events-none disabled:opacity-50"
      >
        {abortPending ? <PendingIndicator /> : <X size={14} />}
        {abortPending && <span>Aborting…</span>}
      </TooltipButton>
    </div>
  )
}
