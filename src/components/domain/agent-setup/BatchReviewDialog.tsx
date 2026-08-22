import { useEffect, useState } from 'react'
import { Loader2 } from 'lucide-react'
import type { ApplyOutcome, InventoryEntry, RedactedCopyPlan } from '@/lib/bindings'
import { partitionBatchCandidates, clientLabel } from '@/lib/agentConfig'
import { useApplyAgentConfigBatch, usePreviewAgentConfigCopy, useUndoAgentConfigCopy } from '@/hooks/useAgentConfig'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { log } from '@/lib/log'
import { PlanReview, ApplyResults } from './CopyPreviewDialog'

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
      for (const { entry, destinations } of candidates) {
        try {
          const outcome = await preview.mutateAsync({ repoId, itemId: entry.itemId, destinations })
          if (outcome.kind === 'ready' && outcome.plan.destinations.length > 0) {
            built.push(outcome.plan)
          } else {
            skippedEntries.push(entry)
          }
        } catch (e) {
          log.error(`building batch preview for ${entry.itemId} failed: ${String(e)}`)
          skippedEntries.push(entry)
        }
      }
      if (cancelled) return
      setPlans(built)
      setSkipped(skippedEntries)
      setStage(built.length > 0 ? 'ready' : 'empty')
    }
    void buildPlans()
    return () => {
      cancelled = true
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps -- build once per dialog open, not on every entries/preview identity change
  }, [])

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
              Nothing could be previewed -- every differing item either has no eligible destination or could not be
              read.
            </p>
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
                      {plan.itemId} <span className="font-normal text-muted-foreground">from {clientLabel(plan.sourceClient)}</span>
                    </div>
                    {planOutcome ? (
                      <ApplyResults
                        results={planOutcome.results}
                        onUndo={(operationId) => undo.mutate(operationId)}
                        undoing={undo.isPending}
                      />
                    ) : (
                      <PlanReview plan={plan} />
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
