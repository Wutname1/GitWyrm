import { useEffect, useState } from 'react'
import { Loader2 } from 'lucide-react'
import type { ApplyOutcome, ClientId, InventoryEntry, RedactedCopyPlan } from '@/lib/bindings'
import { partitionBatchCandidates, clientLabel, itemDisplayName } from '@/lib/agentConfig'
import { useApplyAgentConfigBatch, usePreviewAgentConfigCopy, useUndoAgentConfigCopy } from '@/hooks/useAgentConfig'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { log } from '@/lib/log'
import { PlanReview, ApplyResults } from './CopyPreviewDialog'
import { batchBuildStage } from '@/lib/agentConfig'

type BuildStage = 'building' | 'ready' | 'empty' | 'failed'

/**
 * "Match selected apps" (R7.5/R7.6): every differing item's per-destination
 * plan is computed and shown here, in full, before Apply becomes available --
 * exactly the same `PlanReview` card the single-item `CopyPreviewDialog`
 * shows. Nothing is applied until the user has seen every plan and pressed
 * Apply; applying then reuses `agent_config_apply_batch`, which is built from
 * the same per-plan `agent_config_apply_copy` path as a single copy (task
 * 2.4) -- there is no separate "sync everything" write path, and this dialog
 * adds no new one on the frontend either.
 *
 * Previously `AgentSetupView.handleMatchSelectedApps` called
 * `usePreviewAgentConfigCopy` and `useApplyAgentConfigBatch` back-to-back in
 * the same handler with no UI in between -- every differing item's plan was
 * computed and immediately applied, so the promised "per-item/per-destination
 * preview before Match selected apps can apply anything" (R7.5) never
 * existed. This dialog is the fix: it stops after building the plans and
 * waits for an explicit Apply.
 */
export function BatchReviewDialog({
  entries,
  repoId,
  onClose,
}: {
  entries: InventoryEntry[]
  repoId: string | null
  onClose: () => void
}) {
  const [stage, setStage] = useState<BuildStage>('building')
  const [plans, setPlans] = useState<RedactedCopyPlan[]>([])
  const [skipped, setSkipped] = useState<InventoryEntry[]>([])
  const [outcomes, setOutcomes] = useState<ApplyOutcome[] | null>(null)
  // Which apps each item is going to. The batch used to send every eligible
  // destination without asking, while copying one item made the person pick
  // -- so "match selected apps" could write to an app they never chose. The
  // single flow's rule wins: a destination is included because it was
  // selected, and every item starts with all of its own selected because
  // that is what the button offers to do.
  const [chosen, setChosen] = useState<Record<string, ClientId[]>>({})
  const [rebuild, setRebuild] = useState(0)

  const preview = usePreviewAgentConfigCopy()
  const applyBatch = useApplyAgentConfigBatch(repoId)
  const undo = useUndoAgentConfigCopy(repoId)

  // Build every plan up front, once, when the dialog opens. Each item is
  // previewed individually through the same `agent_config_preview_copy`
  // command the single-item dialog uses -- a batch is several individual
  // previews shown together, never a bulk-preview endpoint of its own.
  useEffect(() => {
    let cancelled = false
    async function buildPlans() {
      const { candidates, skipped: skippedEntries } = partitionBatchCandidates(entries)
      const built: RedactedCopyPlan[] = []
      // Counted so a total failure can be told apart from an empty selection.
      let failures = 0
      let attempted = 0
      for (const { entry, destinations } of candidates) {
        // An item with every destination unticked is not an error and not a
        // skip: the person deliberately left it out, so it simply has no
        // plan this time round.
        const selected = chosen[entry.itemId] ?? destinations
        if (selected.length === 0) continue
        attempted += 1
        try {
          const outcome = await preview.mutateAsync({ repoId, itemId: entry.itemId, destinations: selected })
          if (outcome.kind === 'ready' && outcome.plan.destinations.length > 0) {
            built.push(outcome.plan)
          } else {
            skippedEntries.push(entry)
          }
        } catch (e) {
          failures += 1
          log.error(`building batch preview for ${entry.itemId} failed: ${String(e)}`)
          skippedEntries.push(entry)
        }
      }
      if (cancelled) return
      setPlans(built)
      setSkipped(skippedEntries)
      // `failures` and `attempted` were counted here and never read: the stage
      // collapsed to 'ready' or 'empty', so every attempt failing looked
      // exactly like selecting nothing, under a comment saying the two were
      // told apart. `'failed'` was declared in the union and never set.
      //
      // The distinction matters because the two need different things from
      // the person: an empty selection means tick something, while everything
      // failing means try again or look at what went wrong.
      setStage(batchBuildStage({ built: built.length, attempted, failures }))
    }
    void buildPlans()
    return () => {
      cancelled = true
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps -- rebuilds only when a destination is ticked, not on every entries/preview identity change
  }, [rebuild])

  // What each item COULD go to, from the same partition the build uses, so
  // the checkboxes and the plans can never disagree about eligibility.
  const eligibleFor = (itemId: string): ClientId[] =>
    partitionBatchCandidates(entries).candidates.find((c) => c.entry.itemId === itemId)?.destinations ?? []

  const runApply = () => {
    if (plans.length === 0) return
    applyBatch.mutate(plans.map((p) => p.planId), {
      onSuccess: (result) => setOutcomes(result.outcomes),
      onError: (e) => log.error(`match selected apps apply failed: ${String(e)}`),
    })
  }

  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="flex max-h-[85vh] max-w-2xl flex-col gap-0 overflow-hidden p-0">
        <DialogHeader className="border-b border-border px-4 py-3">
          <DialogTitle className="text-sm">Match selected apps</DialogTitle>
        </DialogHeader>

        <div className="flex-1 overflow-y-auto px-4 py-3">
          {stage === 'building' && (
            <p className="flex items-center gap-2 py-6 text-center text-2xs text-muted-foreground">
              <Loader2 size={13} className="animate-spin" /> Building a preview for every item that differs…
            </p>
          )}

          {stage === 'empty' && (
            <p className="py-6 text-center text-2xs text-muted-foreground">
              Nothing to preview. Every item that differs either has nowhere it can be copied to, or was not ticked.
            </p>
          )}

          {stage === 'failed' && (
            <div className="flex flex-col items-center gap-2 py-6 text-center">
              <p className="text-2xs text-[var(--gw-amber)]">
                None of these could be prepared. Nothing has been changed.
              </p>
              <button
                type="button"
                onClick={() => {
                  // The same counter that reruns the build when a destination
                  // is ticked; retrying is the same operation.
                  setStage('building')
                  setRebuild((n) => n + 1)
                }}
                className="rounded border border-border px-2 py-1 text-2xs font-semibold text-foreground hover:bg-panel2"
              >
                Try again
              </button>
            </div>
          )}

          {(stage === 'ready' || outcomes) && (
            <div className="flex flex-col gap-4">
              {skipped.length > 0 && !outcomes && (
                <p className="text-2xs text-muted-foreground">
                  {skipped.length} item{skipped.length === 1 ? '' : 's'} skipped (no eligible destination or could
                  not be previewed): {skipped.map((e) => e.displayName).join(', ')}
                </p>
              )}
              {plans.map((plan) => {
                const planOutcome = outcomes?.find((o) => o.planId === plan.planId)
                return (
                  <div key={plan.planId} className="rounded-md border border-border p-2.5">
                    <div className="mb-2 text-xs font-semibold text-foreground">
                      {/* The plan carries only the internal id; the name lives
                          on `entries`, which is already a prop here. */}
                      {itemDisplayName(entries, plan.itemId)}{' '}
                      <span className="font-normal text-muted-foreground">from {clientLabel(plan.sourceClient)}</span>
                    </div>
                    {planOutcome ? (
                      <ApplyResults
                        results={planOutcome.results}
                        onUndo={(operationId) => undo.mutate(operationId)}
                        undoing={undo.isPending}
                      />
                    ) : (
                      <>
                        <DestinationPicker
                          itemId={plan.itemId}
                          eligible={eligibleFor(plan.itemId)}
                          selected={chosen[plan.itemId] ?? eligibleFor(plan.itemId)}
                          disabled={applyBatch.isPending}
                          onChange={(next) => {
                            setChosen((prev) => ({ ...prev, [plan.itemId]: next }))
                            setStage('building')
                            setRebuild((n) => n + 1)
                          }}
                        />
                        <PlanReview plan={plan} />
                      </>
                    )}
                  </div>
                )
              })}
            </div>
          )}
        </div>

        <div className="flex items-center justify-end gap-2 border-t border-border px-4 py-3">
          <Button variant="secondary" size="sm" onClick={onClose}>
            {outcomes ? 'Close' : 'Cancel'}
          </Button>
          {stage === 'ready' && !outcomes && (
            // Said before the decision, not only in the results afterwards.
            // Each file is written safely on its own -- saved first, and
            // skipped if it changed since the preview -- but the batch is not
            // one all-or-nothing operation, so a failure partway leaves some
            // apps updated and others not.
            <p className="mr-auto max-w-[24rem] text-2xs leading-relaxed text-muted-foreground">
              Each file is saved first and skipped if it changed since this preview. They are copied one at a time, so
              if one fails the earlier ones stay copied.
            </p>
          )}
          {stage === 'ready' && !outcomes && (
            <Button size="sm" onClick={runApply} disabled={applyBatch.isPending || plans.length === 0}>
              {applyBatch.isPending ? <Loader2 size={13} className="animate-spin" /> : null}
              Apply {plans.length} plan{plans.length === 1 ? '' : 's'}
            </Button>
          )}
        </div>
      </DialogContent>
    </Dialog>
  )
}

/**
 * Which apps one item is copied to.
 *
 * The same explicit choice the single-item flow asks for. Unticking the last
 * destination leaves the item out of the batch entirely rather than sending
 * it somewhere by default: "match selected apps" should never write to an
 * app nobody selected.
 */
function DestinationPicker({
  itemId,
  eligible,
  selected,
  disabled,
  onChange,
}: {
  itemId: string
  eligible: ClientId[]
  selected: ClientId[]
  disabled?: boolean
  onChange: (next: ClientId[]) => void
}) {
  if (eligible.length <= 1) return null
  return (
    <div className="mb-2 flex flex-wrap items-center gap-x-3 gap-y-1">
      <span className="text-2xs font-semibold text-sub">Copy to</span>
      {eligible.map((client) => {
        const on = selected.includes(client)
        return (
          <label key={client} className="flex cursor-pointer items-center gap-1.5 text-2xs text-sub hover:text-foreground">
            <input
              type="checkbox"
              checked={on}
              disabled={disabled}
              onChange={() =>
                onChange(on ? selected.filter((c) => c !== client) : [...selected, client])
              }
              className="size-3 accent-[var(--gw-accent)]"
              aria-label={`Copy ${itemId} to ${clientLabel(client)}`}
            />
            {clientLabel(client)}
          </label>
        )
      })}
    </div>
  )
}
