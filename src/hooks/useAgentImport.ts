import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { commands } from '@/lib/bindings'
import { keys, unwrap } from '@/lib/queryKeys'

/**
 * External chat import (agent-desk-external-chat-import): detected clients,
 * per-adapter scan/import, and the honest "continue" actions.
 *
 * Kept in its own hook file rather than folded into `useAgentSessions.ts` --
 * import is a separate concern from the native session list/detail, has its
 * own query keys, and its mutations invalidate the *session* list/detail
 * queries from `useAgentSessions.ts` without needing to import that file's
 * internals.
 */

/** Every adapter GitWyrm ships, with detection state and whether its
 * capability flag is on (`commands::agent_import::ADAPTER_CAPABILITY_FLAGS`).
 * A disabled adapter (e.g. OpenChamber, pending a verified schema) still
 * appears here so the UI can say why it is not offered, rather than omitting
 * it silently. */
export function useAgentImportAdapters() {
  return useQuery({
    queryKey: keys.agentImportAdapters,
    queryFn: async () => unwrap(await commands.agentImportListAdapters()),
    // Detection touches the filesystem across up to five clients; a session
    // rarely needs it refreshed more than once every few minutes.
    staleTime: 2 * 60 * 1000,
  })
}

/** One adapter's scanned external sessions, each carrying its own project
 * reconciliation result and whether it was already imported. `enabled`
 * mirrors the adapter's own capability flag so a disabled adapter's picker
 * never fires a request that only comes back `AdapterDisabled`. */
export function useAgentImportScan(adapterId: string | null, enabled: boolean) {
  return useQuery({
    queryKey: keys.agentImportScan(adapterId ?? 'none'),
    enabled: adapterId != null && enabled,
    queryFn: async () => unwrap(await commands.agentImportScan(adapterId!)),
  })
}

/** Whether/how "Continue externally" can work for one external session,
 * queried lazily (only once a session row actually renders that action) so
 * scanning a list of 100 sessions does not fire 100 extra probes. */
export function useAgentImportContinuation(
  adapterId: string | null,
  externalSessionId: string | null
) {
  return useQuery({
    queryKey: keys.agentImportContinuation(adapterId ?? 'none', externalSessionId ?? 'none'),
    enabled: adapterId != null && externalSessionId != null,
    queryFn: async () =>
      unwrap(await commands.agentImportContinuationCapability(adapterId!, externalSessionId!)),
  })
}

/** Import (or incrementally refresh) one external session into a durable
 * GitWyrm session. Invalidates the adapter's scan (so `alreadyImported`
 * flips) and the native session list (the new/updated session must appear
 * there immediately, per Rule #1: every action needs a visible result). */
export function useImportExternalSession() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: async ({
      adapterId,
      externalSessionId,
    }: {
      adapterId: string
      externalSessionId: string
    }) => unwrap(await commands.agentImportSession(adapterId, externalSessionId)),
    onSuccess: (_result, variables) => {
      qc.invalidateQueries({ queryKey: keys.agentImportScan(variables.adapterId) })
      qc.invalidateQueries({ queryKey: keys.agentSessionsAll })
    },
  })
}

/** "Continue here" (task 4.4): append a native handoff segment to an
 * imported session. Invalidates that session's own query (the new segment
 * must render immediately) and the session list (title/updated_at/state
 * changed). */
export function useContinueImportedSessionHere() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: async (sessionId: string) =>
      unwrap(await commands.agentImportContinueHere(sessionId)),
    onSuccess: (_result, sessionId) => {
      qc.invalidateQueries({ queryKey: keys.agentSession(sessionId) })
      qc.invalidateQueries({ queryKey: keys.agentSessionsAll })
    },
  })
}

/** "Unlink from <client>" (task 4.3): forget the tie between an imported
 * session and its external source while keeping every imported message.
 * Invalidates the session (the unlink note must render), the session list
 * (updated_at moved), the adapter's scan (the row flips back to "Import",
 * since a later import makes a new chat), and that external session's
 * continuation probe (the row no longer offers to open the client). */
export function useUnlinkImportedSession() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: async ({ sessionId }: { sessionId: string; adapterId: string; externalSessionId: string }) =>
      unwrap(await commands.agentImportUnlink(sessionId)),
    onSuccess: (_result, { sessionId, adapterId, externalSessionId }) => {
      qc.invalidateQueries({ queryKey: keys.agentSession(sessionId) })
      qc.invalidateQueries({ queryKey: keys.agentSessionsAll })
      qc.invalidateQueries({ queryKey: keys.agentImportScan(adapterId) })
      qc.invalidateQueries({ queryKey: keys.agentImportContinuation(adapterId, externalSessionId) })
    },
  })
}
