import { TriangleAlert, X } from 'lucide-react'
import { Button } from '@/components/ui/button'
import type { StartFailureAction, StartFailureCard as StartFailureCardModel } from '@/lib/agentDeskStartFailure'

const LABELS: Record<StartFailureAction, string> = {
  openProject: 'Open project',
  pickProvider: 'Pick another AI tool',
  tryAgain: 'Try again',
}

/**
 * The card that stays on screen after a run failed to start (source-kickoffs
 * tasks 2.4 and 5.3). Rendered above the composer for message sends and
 * inside the plan card for graph starts; both feed it the same model from
 * `agentDeskStartFailure.ts`, so the copy and the offered actions are
 * decided once and tested once.
 *
 * Every button changes something visible: Try again flips to "Starting…"
 * and either clears the card (success) or replaces it (a new failure);
 * Open project and Pick another AI tool are handed to the owner, which
 * opens the repository or the provider picker. Close removes the card.
 */
export function StartFailureCard({
  card,
  busy,
  onAction,
  onDismiss,
}: {
  card: StartFailureCardModel
  /** True while a retry or project open is in flight; disables every button. */
  busy: boolean
  onAction: (action: StartFailureAction) => void
  onDismiss: () => void
}) {
  return (
    <div
      role="alert"
      className="flex items-start gap-2 rounded-md border border-amber-600/40 bg-amber-500/10 px-2.5 py-2 text-2xs leading-relaxed"
    >
      <TriangleAlert size={13} className="mt-px flex-none text-amber-700 dark:text-amber-400" aria-hidden />
      <div className="min-w-0 flex-1">
        <div className="font-semibold text-foreground">{card.title}</div>
        <p className="mt-0.5 text-foreground/90">{card.body}</p>
        {card.detail && (
          <p className="mt-1 break-words rounded bg-panel2 px-1.5 py-1 font-mono text-[10px] text-muted-foreground">
            {card.detail}
          </p>
        )}
        <div className="mt-1.5 flex flex-wrap gap-1.5">
          {card.actions.map((action, index) => (
            <Button
              key={action}
              type="button"
              size="xs"
              // The first action is the one most likely to fix it, so it gets
              // the filled style; the rest sit beside it as plain buttons.
              variant={index === 0 ? 'default' : 'outline'}
              disabled={busy}
              onClick={() => onAction(action)}
            >
              {busy && action === 'tryAgain' ? 'Starting…' : LABELS[action]}
            </Button>
          ))}
        </div>
      </div>
      <button
        type="button"
        onClick={onDismiss}
        disabled={busy}
        aria-label="Close"
        className="flex h-5 w-5 flex-none items-center justify-center rounded text-muted-foreground hover:bg-panel3 hover:text-foreground disabled:opacity-50"
      >
        <X size={12} />
      </button>
    </div>
  )
}
