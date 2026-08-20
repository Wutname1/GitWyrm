import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { commands, type ClientId } from '@/lib/bindings'
import { keys, unwrap } from '@/lib/queryKeys'

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
    onSettled: () => {
      qc.invalidateQueries({ queryKey: keys.agentConfigInventory(repoId) })
    },
  })
}

/** Undo one operation by ID (task 3.4, 2.5's "operation receipt with Undo"). */
export function useUndoAgentConfigCopy(repoId: string | null) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: async (operationId: string) => unwrap(await commands.agentConfigUndo(operationId)),
    onSettled: () => {
      qc.invalidateQueries({ queryKey: keys.agentConfigInventory(repoId) })
    },
  })
}
