import { History, Loader2, Undo2 } from 'lucide-react'
import { useAgentConfigRecentOperations, useUndoAgentConfigCopy } from '@/hooks/useAgentConfig'
import { describeConfigOperation } from '@/lib/agentConfig'
import { describeAge } from '@/lib/agentDeskSources'

/**
 * Changes GitWyrm has made to other apps' settings, newest first, each with
 * its own Undo.
 *
 * Undo has always existed and receipts have always been written to disk, but
 * the operation id lived only in the apply dialog's own state -- so closing
 * that dialog made the write permanent in practice. The command that lists
 * receipts was added specifically to close that gap and then had no caller
 * anywhere, leaving the vision's "receipt and Undo" with only one half built.
 *
 * A change that has already been put back stays in the list, marked, rather
 * than vanishing: the record of what happened is the point, and a row that
 * disappears on Undo reads as though it never happened.
 */
/**
 * When the change was made. Reuses the app's one age vocabulary rather than
 * inventing a second phrasing for the same idea.
 */
function RelativeApplied({ at }: { at: string }) {
  const then = Date.parse(at)
  if (Number.isNaN(then)) return null
  return <p className="text-2xs text-muted-foreground">Made {describeAge(Date.now() - then)}</p>
}

export function RecentConfigChanges({ repoId }: { repoId: string | null }) {
  const receipts = useAgentConfigRecentOperations()
  const undo = useUndoAgentConfigCopy(repoId)
  const rows = receipts.data ?? []

  if (receipts.isLoading) {
    return (
      <p className="flex items-center gap-1.5 px-3 py-2 text-2xs text-muted-foreground">
        <Loader2 size={11} className="animate-spin" aria-hidden />
        Looking for recent changes…
      </p>
    )
  }

  // Nothing written yet is not a problem to report, so this says so quietly
  // rather than rendering an empty frame.
  if (rows.length === 0) {
    return <p className="px-3 py-2 text-2xs text-muted-foreground">GitWyrm has not changed any app's settings.</p>
  }

  return (
    <div className="flex flex-col gap-1">
      <div className="flex items-center gap-1.5 px-3 pt-2 text-2xs font-bold uppercase tracking-wide text-muted-foreground">
        <History size={11} aria-hidden />
        Changes GitWyrm made
      </div>
      <ul className="flex flex-col">
        {rows.map((r) => (
          <li key={r.operationId} className="flex items-start gap-2 px-3 py-1.5">
            <div className="min-w-0 flex-1">
              <p className="truncate text-2xs text-foreground">{describeConfigOperation(r)}</p>
              <p className="truncate font-mono text-2xs text-sub" title={r.destinationPath}>
                {r.destinationPath}
              </p>
              <RelativeApplied at={r.appliedAt} />
            </div>
            {!r.undone && (
              <button
                type="button"
                onClick={() => undo.mutate(r.operationId)}
                disabled={undo.isPending}
                className="flex flex-none items-center gap-1 rounded border border-border px-1.5 py-1 text-2xs font-semibold text-foreground hover:bg-panel2 disabled:cursor-not-allowed disabled:opacity-50"
              >
                <Undo2 size={10} aria-hidden />
                Put it back
              </button>
            )}
          </li>
        ))}
      </ul>
    </div>
  )
}
