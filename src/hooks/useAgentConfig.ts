import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { toast } from 'sonner'
import { commands, type ClientId } from '@/lib/bindings'
import { keys, unwrap } from '@/lib/queryKeys'
import { explainConfigUndoOutcome } from '@/lib/agentConfig'
import { describeError, log } from '@/lib/log'

/**
 * Agent Setup: read-only inventory scan (architecture.md section 13,
 * `agent_config_scan`). Never mutates a client's configuration -- see the
 * spec's "Configuration discovery is separate from writing".
 */
export function useAgentConfigInventory(repoId: string | null) {
  return useQuery({
    queryKey: keys.agentConfigInventory(repoId),
    queryFn: async () => unwrap(await commands.agentConfigScan(repoId)),
  })
}

export function useAgentConfigDetectedClients(repoId: string | null) {
  return useQuery({
    queryKey: keys.agentConfigDetectedClients(repoId),
    queryFn: async () => unwrap(await commands.agentConfigDetectClients(repoId)),
  })
}

/**
 * Compute a not-yet-applied preview for one item (task 2.2/2.3, 3.1). The
 * caller reviews `plan.destinations` -- each with its own proposed content,
 * warnings, and before-hash -- before ever calling
 * `useApplyAgentConfigCopy`.
 */
export function usePreviewAgentConfigCopy() {
  return useMutation({
    mutationFn: async ({
      repoId,
      itemId,
      destinations,
    }: {
      repoId: string | null
      itemId: string
      destinations: ClientId[]
    }) => unwrap(await commands.agentConfigPreviewCopy(repoId, itemId, destinations)),
  })
}

/**
 * Apply one previously previewed plan (task 3.2-3.4). Invalidates the
 * inventory afterward so per-client state reflects the write immediately
 * (task 2.5: "Show immediate pending/success/failure").
 */
export function useApplyAgentConfigCopy(repoId: string | null) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: async (planId: string) => unwrap(await commands.agentConfigApplyCopy(planId)),
    onSettled: () => {
      qc.invalidateQueries({ queryKey: keys.agentConfigInventory(repoId) })
      // The receipt list shows an `undone` flag, so it has to be refetched or
      // a row the user just undid keeps offering Undo.
      qc.invalidateQueries({ queryKey: keys.agentConfigRecentOperations() })
    },
  })
}

/**
 * "Match selected apps": apply several already-previewed plans in one batch
 * (task 2.4) -- built from the same per-item plans as a single apply, never
 * a separate hidden overwrite path.
 */
export function useApplyAgentConfigBatch(repoId: string | null) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: async (planIds: string[]) => unwrap(await commands.agentConfigApplyBatch({ planIds })),
    // A failure here used to be completely silent: the button stopped spinning
    // and nothing else happened, so a copy that did not run looked identical
    // to one that did. `useUndoAgentConfigCopy` below reports both outcomes;
    // this member of the same family simply omitted it.
    onError: (e) => {
      log.error(`agent config batch apply failed: ${describeError(e)}`)
      toast.error('Those settings could not be copied.', {
        description: 'Nothing was changed. You can try again.',
      })
    },
    onSettled: () => {
      qc.invalidateQueries({ queryKey: keys.agentConfigInventory(repoId) })
      qc.invalidateQueries({ queryKey: keys.agentConfigRecentOperations() })
    },
  })
}

/** Undo one operation by ID (task 3.4, 2.5's "operation receipt with Undo"). */
export function useUndoAgentConfigCopy(repoId: string | null) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: async (operationId: string) => unwrap(await commands.agentConfigUndo(operationId)),
    // The outcome used to be discarded, so a refused undo looked exactly like
    // a successful one -- the person believed another app's config was back to
    // how it was when it may not have been touched at all. The refusal case
    // that matters most is `concurrentChangeRefused`: the file changed after
    // the copy, so putting it back would clobber that newer edit.
    onSuccess: (outcome) => {
      const { message, restored } = explainConfigUndoOutcome(outcome)
      if (restored) toast.success(message)
      else toast.warning(message)
    },
    onSettled: () => {
      qc.invalidateQueries({ queryKey: keys.agentConfigInventory(repoId) })
    },
  })
}

/**
 * Config changes GitWyrm has made, newest first, so Undo outlives the dialog.
 *
 * `agent_config_undo` has always taken an operation id and receipts have always
 * been written to disk, but the id existed only in the apply dialog's own
 * state -- close the dialog and the write was permanent in practice. The
 * backend command that lists receipts was added to close exactly that gap and
 * then had no caller at all, so the vision's "receipt and Undo" was still
 * missing its second half.
 */
export function useAgentConfigRecentOperations() {
  return useQuery({
    queryKey: keys.agentConfigRecentOperations(),
    queryFn: async () => unwrap(await commands.agentConfigRecentOperations()),
  })
}
