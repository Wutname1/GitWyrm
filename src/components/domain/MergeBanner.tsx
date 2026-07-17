import { AlertTriangle, GitMerge, X } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { useMergeState } from '@/hooks/useGitQueries'
import { useGitMutations } from '@/hooks/useGitMutations'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo } from '@/stores/workspaceStore'

/** Cleans up the state message's first line into just the branch/commit label. */
function incomingName(label: string | null | undefined): string {
  if (!label) return 'incoming changes'
  const match = label.match(/Merge (?:branch|remote-tracking branch) '([^']+)'/)
  return match ? match[1] : label
}

export function MergeBanner() {
  const repo = useActiveRepo()
  const merge = useMergeState(repo?.id ?? null)
  const m = useGitMutations(repo?.id ?? null)
  const openConflict = useUiStore((s) => s.openConflict)

  const state = merge.data
  if (!state?.merging) return null

  const isCherryPick = state.operation === 'CherryPick'
  const verb = isCherryPick ? 'Cherry-picking' : 'Merging'
  const finishLabel = isCherryPick ? 'Commit pick' : 'Commit merge'
  const abortTitle = isCherryPick ? 'Abort cherry-pick' : 'Abort merge'

  const conflicts = state.conflicts
  const remaining = conflicts.length
  const label = incomingName(state.incoming_label)

  const commit = () => {
    const fallback = isCherryPick ? label : `Merge ${label}`
    const message = state.incoming_label?.trim() || fallback
    m.commitMerge.mutate(message)
  }

  return (
    <div className="flex flex-none items-center gap-3 border-b border-modified/40 bg-modified/[.08] px-3.5 py-2">
      <GitMerge size={15} className="flex-none text-modified" strokeWidth={2} />
      <div className="min-w-0 flex-1">
        <div className="text-[11.5px] font-semibold text-foreground">
          {verb} <span className="font-mono text-modified">{label}</span>
        </div>
        <div className="flex items-center gap-1.5 text-[10.5px] text-sub">
          {remaining > 0 ? (
            <>
              <AlertTriangle size={11} className="text-removed" />
              {remaining} conflict{remaining === 1 ? '' : 's'} left to resolve
            </>
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
        disabled={remaining > 0 || m.commitMerge.isPending}
        onClick={commit}
        className="h-7 flex-none text-[11px]"
      >
        {m.commitMerge.isPending ? 'Committing…' : finishLabel}
      </Button>

      <button
        onClick={() => m.abortMerge.mutate()}
        title={abortTitle}
        disabled={m.abortMerge.isPending}
        className="flex size-7 flex-none items-center justify-center rounded border border-border bg-panel2 text-sub hover:border-muted-foreground hover:bg-panel3"
      >
        <X size={14} />
      </button>
    </div>
  )
}
