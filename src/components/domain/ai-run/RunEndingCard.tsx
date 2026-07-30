import { Button } from '@/components/ui/button'
import type { RunState } from '@/lib/bindings'

export interface EndingActions {
  onKeep: () => void
  onUndo: () => void
  onRestart: () => void
  /** Only offered on failure: try the same task again with an added note. */
  onRetryWithNote?: () => void
  /** Only offered when the cause was a sign-in problem. */
  onReconnect?: () => void
  /** The escape hatch: copy the task out to another tool. */
  onCopyHandoff?: () => void
}

/**
 * What is offered once a run has ended.
 *
 * Every ending says the same two things first: nothing was committed, and the
 * user's own work is untouched. A dead end that does not say what is safe
 * leaves people guessing about their own files, which is the worst moment to
 * be guessing.
 */
export function RunEndingCard({
  state,
  detail,
  actions,
  needsReconnect,
}: {
  state: RunState
  detail: string
  actions: EndingActions
  needsReconnect?: boolean
}) {
  const failed = state === 'failed'
  return (
    <div
      className={
        failed
          ? 'rounded-lg border border-destructive/40 bg-destructive/5 p-3'
          : 'rounded-lg border border-border bg-panel2 p-3'
      }
    >
      <div className="text-xs font-semibold text-foreground">
        {/* Three endings, three titles. Calling a finished run "stopped" reads
            as though something went wrong when nothing did. */}
        {state === 'failed'
          ? "This didn't finish"
          : state === 'stopped'
            ? 'Run stopped'
            : 'Task finished'}
      </div>
      <p className="mt-1 text-2xs leading-relaxed text-sub">{detail}</p>
      {/* Said once, not twice: the first draft claimed the edits were
          uncommitted and then that nothing was committed, which is the same
          fact stated as though it were two. */}
      <p className="mt-1 text-2xs leading-relaxed text-muted-foreground">
        The AI's edits are waiting in your changes list, not committed. Your own work is
        untouched.
      </p>

      <div className="mt-2.5 flex flex-wrap gap-1.5">
        <Button size="sm" className="h-7 text-xs" onClick={actions.onKeep}>
          Keep the changes
        </Button>
        <Button
          variant="secondary"
          size="sm"
          className="h-7 text-xs"
          onClick={actions.onUndo}
        >
          Undo the AI's edits
        </Button>
        <Button
          variant="ghost"
          size="sm"
          className="h-7 text-xs text-muted-foreground hover:text-foreground"
          onClick={actions.onRestart}
        >
          Restart this task
        </Button>
        {failed && actions.onRetryWithNote && (
          <Button
            variant="ghost"
            size="sm"
            className="h-7 text-xs text-muted-foreground hover:text-foreground"
            onClick={actions.onRetryWithNote}
          >
            Try again with a note
          </Button>
        )}
      </div>

      {needsReconnect && (
        <div className="mt-2.5 border-t border-border pt-2.5">
          <p className="text-2xs text-muted-foreground">
            Your AI sign-in needs renewing. You can also send this task to another tool
            instead.
          </p>
          <div className="mt-1.5 flex flex-wrap gap-1.5">
            {actions.onReconnect && (
              <Button size="sm" className="h-7 text-xs" onClick={actions.onReconnect}>
                Reconnect
              </Button>
            )}
            {actions.onCopyHandoff && (
              <Button
                variant="secondary"
                size="sm"
                className="h-7 text-xs"
                onClick={actions.onCopyHandoff}
              >
                Copy for another tool
              </Button>
            )}
          </div>
        </div>
      )}
    </div>
  )
}
