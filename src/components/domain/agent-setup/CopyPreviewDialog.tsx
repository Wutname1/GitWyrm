import { useState } from 'react'
import { toast } from 'sonner'
import { AlertTriangle, CheckCircle2, Loader2, RotateCcw } from 'lucide-react'
import type { ClientId, RedactedCopyPlan, DestinationApplyResult, InventoryEntry } from '@/lib/bindings'
import { CLIENT_COLUMN_ORDER, clientLabel, eligibleDestinationsFor, explainPreviewRefusal } from '@/lib/agentConfig'
import { useApplyAgentConfigCopy, usePreviewAgentConfigCopy, useUndoAgentConfigCopy } from '@/hooks/useAgentConfig'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { log } from '@/lib/log'
import { cn } from '@/lib/utils'

/**
 * Per-item preview/apply flow (tasks 2.2, 2.3, 2.5, 3.1).
 *
 * The user first picks destinations, then reviews an exact per-destination
 * preview -- proposed content is never shown as a raw diff of file bytes;
 * `redactedDiffSummary` names the fields that changed with secret values
 * already redacted server-side (task 1.5). Nothing is written until Apply is
 * pressed on a plan the user has seen. Rule #3: no typed confirmation is
 * used anywhere in this flow -- the preview step plus Undo afterward is the
 * safety net.
 */
export function CopyPreviewDialog({
  entry,
  repoId,
  onClose,
}: {
  entry: InventoryEntry
  repoId: string | null
  onClose: () => void
}) {
  const eligible = eligibleDestinationsFor(entry)
  const [selected, setSelected] = useState<Set<ClientId>>(new Set(eligible))
  const [plan, setPlan] = useState<RedactedCopyPlan | null>(null)
  const [results, setResults] = useState<DestinationApplyResult[] | null>(null)

  const preview = usePreviewAgentConfigCopy()
  const apply = useApplyAgentConfigCopy(repoId)
  const undo = useUndoAgentConfigCopy(repoId)

  const runPreview = () => {
    setResults(null)
    preview.mutate(
      { repoId, itemId: entry.itemId, destinations: Array.from(selected) },
      {
        onSuccess: (outcome) => {
          if (outcome.kind === 'ready') {
            setPlan(outcome.plan)
          } else {
            // Both refusals used to land here silently, putting the person
            // back on the destination picker as though they had not chosen --
            // so they choose the same thing again and get the same nothing.
            setPlan(null)
            toast.error('Nothing to preview.', { description: explainPreviewRefusal(outcome.kind) })
          }
        },
        onError: (e) => {
          log.error(`agent config preview failed: ${String(e)}`)
          toast.error('That copy could not be worked out.', {
            description: 'Nothing has been changed. You can try again.',
          })
        },
      }
    )
  }

  const runApply = () => {
    if (!plan) return
    apply.mutate(plan.planId, {
      onSuccess: (outcome) => setResults(outcome.results),
      // The batch path reports its failures; this single-item sibling was
      // left log-only, so one copy failing said nothing at all.
      onError: (e) => {
        log.error(`agent config apply failed: ${String(e)}`)
        toast.error('Those settings could not be copied.', {
          description: 'Nothing was changed. You can try again.',
        })
      },
    })
  }

  const toggleDestination = (client: ClientId) => {
    setSelected((prev) => {
      const next = new Set(prev)
      if (next.has(client)) next.delete(client)
      else next.add(client)
      return next
    })
  }

  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="flex max-h-[85vh] max-w-2xl flex-col gap-0 overflow-hidden p-0">
        <DialogHeader className="border-b border-border px-4 py-3">
          <DialogTitle className="text-sm">Copy &ldquo;{entry.displayName}&rdquo;</DialogTitle>
        </DialogHeader>

        <div className="flex-1 overflow-y-auto px-4 py-3">
          {!plan && (
            <DestinationPicker
              entry={entry}
              eligible={eligible}
              selected={selected}
              onToggle={toggleDestination}
            />
          )}

          {plan && !results && <PlanReview plan={plan} />}

          {results && <ApplyResults results={results} onUndo={(operationId) => undo.mutate(operationId)} undoing={undo.isPending} />}
        </div>

        <div className="flex items-center justify-end gap-2 border-t border-border px-4 py-3">
          <Button variant="secondary" size="sm" onClick={onClose}>
            Close
          </Button>
          {!plan && (
            <Button size="sm" onClick={runPreview} disabled={selected.size === 0 || preview.isPending}>
              {preview.isPending ? <Loader2 size={13} className="animate-spin" /> : null}
              Preview copy
            </Button>
          )}
          {plan && !results && (
            <p className="mr-auto max-w-[22rem] text-2xs leading-relaxed text-muted-foreground">
              Your current file is saved first, so you can put it back. If it changed since this preview, nothing is
              written.
            </p>
          )}
          {plan && !results && (
            <Button size="sm" onClick={runApply} disabled={apply.isPending}>
              {apply.isPending ? <Loader2 size={13} className="animate-spin" /> : null}
              Apply
            </Button>
          )}
        </div>
      </DialogContent>
    </Dialog>
  )
}

function DestinationPicker({
  entry,
  eligible,
  selected,
  onToggle,
}: {
  entry: InventoryEntry
  eligible: ClientId[]
  selected: Set<ClientId>
  onToggle: (client: ClientId) => void
}) {
  if (eligible.length === 0) {
    return (
      <p className="text-2xs text-muted-foreground">
        Every other app already matches this item, or has no supported way to receive it yet.
      </p>
    )
  }
  return (
    <div className="flex flex-col gap-2">
      <p className="text-2xs text-muted-foreground">Choose which apps should receive this item.</p>
      {CLIENT_COLUMN_ORDER.filter((c) => eligible.includes(c)).map((client) => {
        const status = entry.perClient.find((s) => s.client === client)
        return (
          <label
            key={client}
            className="flex cursor-pointer items-center gap-2 rounded-md border border-border px-2.5 py-1.5 hover:bg-panel2"
          >
            <input
              type="checkbox"
              checked={selected.has(client)}
              onChange={() => onToggle(client)}
              className="size-3.5 accent-[var(--gw-accent)]"
            />
            <span className="text-xs font-medium text-foreground">{clientLabel(client)}</span>
            <span className="ml-auto text-2xs text-muted-foreground">{status?.state}</span>
          </label>
        )
      })}
    </div>
  )
}

/**
 * Exported so `BatchReviewDialog` (the "Match selected apps" flow) can render
 * the exact same per-destination preview card for every plan in a batch --
 * there is no separate, lighter-weight preview rendering for the batch path
 * (task R7.5/R7.6: the same per-item plan review gates a batch as gates a
 * single copy).
 */
export function PlanReview({ plan }: { plan: RedactedCopyPlan }) {
  return (
    <div className="flex flex-col gap-3">
      {plan.destinations.length === 0 && (
        <p className="text-2xs text-muted-foreground">No destination could be previewed.</p>
      )}
      {plan.destinations.map((dest) => (
        <div key={dest.client} className="rounded-md border border-border p-2.5">
          <div className="mb-1.5 flex items-center justify-between">
            <span className="text-xs font-semibold text-foreground">{clientLabel(dest.client)}</span>
            <span className="truncate text-2xs text-muted-foreground">{dest.destinationPath}</span>
          </div>

          {dest.warnings.length > 0 && (
            <ul className="mb-1.5 flex flex-col gap-1">
              {dest.warnings.map((w, i) => (
                <li
                  key={i}
                  // The secret IS written to the destination file -- the
                  // backend's own test asserts it, because a connector copied
                  // without its credential does not work.
                  //
                  // This branch used to render green with a check shield,
                  // under a comment saying GitWyrm was "deliberately leaving a
                  // credential behind". It is not: the variant was called
                  // `SecretNotCopied` and the frontend believed the name. A
                  // person read a reassurance while their token was being
                  // handed to another application. Amber, because handing a
                  // credential somewhere is worth a second look, not a
                  // celebration.
                  className={cn(
                    'flex items-start gap-1.5 rounded px-1.5 py-1 text-2xs',
                    w.kind === 'secretWillBeCopied'
                      ? 'bg-[color-mix(in_srgb,var(--gw-amber)_14%,transparent)] text-[var(--gw-amber)]'
                      : 'text-muted-foreground'
                  )}
                >
                  <AlertTriangle size={12} className="mt-px flex-none" aria-hidden />
                  <span>{w.message}</span>
                </li>
              ))}
            </ul>
          )}

          {dest.redactedDiffSummary.length > 0 ? (
            <ul className="flex flex-col gap-0.5">
              {dest.redactedDiffSummary
                .filter((line) => line.change !== 'unchanged')
                .map((line, i) => (
                  <li key={i} className="flex items-center gap-1.5 text-2xs">
                    <span
                      className={cn(
                        'inline-block w-12 flex-none rounded px-1 text-center font-medium',
                        line.change === 'added'
                          ? 'bg-[color-mix(in_srgb,var(--gw-green)_16%,transparent)] text-[color-mix(in_srgb,var(--gw-green)_85%,var(--gw-text))]'
                          : 'bg-[color-mix(in_srgb,var(--gw-amber)_16%,transparent)] text-[color-mix(in_srgb,var(--gw-amber)_85%,var(--gw-text))]'
                      )}
                    >
                      {line.change === 'added' ? 'new' : 'change'}
                    </span>
                    <code className="truncate text-muted-foreground">{line.fieldPath}</code>
                  </li>
                ))}
            </ul>
          ) : (
            <p className="text-2xs text-muted-foreground">No field-level changes to show.</p>
          )}
        </div>
      ))}
    </div>
  )
}

/** Exported for the same reason as `PlanReview` -- see its doc comment. */
export function ApplyResults({
  results,
  onUndo,
  undoing,
}: {
  results: DestinationApplyResult[]
  onUndo: (operationId: string) => void
  undoing: boolean
}) {
  return (
    <div className="flex flex-col gap-2">
      {results.map((result, i) => {
        if (result.kind === 'applied') {
          return (
            <div key={i} className="flex items-center gap-2 rounded-md border border-border px-2.5 py-2">
              <CheckCircle2 size={14} className="flex-none text-[var(--gw-green)]" aria-hidden />
              <span className="flex-1 text-xs text-foreground">{clientLabel(result.client)} updated.</span>
              <Button
                size="xs"
                variant="secondary"
                disabled={undoing}
                onClick={() => onUndo(result.operationId)}
              >
                <RotateCcw size={11} /> Undo
              </Button>
            </div>
          )
        }
        if (result.kind === 'concurrentChangeRefused') {
          return (
            <div key={i} className="flex items-start gap-2 rounded-md border border-border bg-[color-mix(in_srgb,var(--gw-amber)_10%,transparent)] px-2.5 py-2">
              <AlertTriangle size={14} className="mt-px flex-none text-[var(--gw-amber)]" aria-hidden />
              <span className="text-xs text-foreground">
                {clientLabel(result.client)}&rsquo;s file changed since the preview, so nothing was written. Refresh
                and try again.
              </span>
            </div>
          )
        }
        return (
          <div key={i} className="flex items-start gap-2 rounded-md border border-border bg-[color-mix(in_srgb,var(--gw-red)_10%,transparent)] px-2.5 py-2">
            <AlertTriangle size={14} className="mt-px flex-none text-[var(--gw-red)]" aria-hidden />
            <span className="text-xs text-foreground">
              {clientLabel(result.client)} could not be updated: {result.detail}
            </span>
          </div>
        )
      })}
    </div>
  )
}
