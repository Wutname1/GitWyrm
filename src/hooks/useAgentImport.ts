import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { commands } from '@/lib/bindings'
import { keys, unwrap } from '@/lib/queryKeys'
import { IMPORT_SCAN_STALE_MS, importedSessionId } from '@/lib/agentImportDisplay'
import { describeError, log } from '@/lib/log'
import { toast } from 'sonner'

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
    staleTime: IMPORT_SCAN_STALE_MS,
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
    // Same reasoning as the adapter list above, and more so: a scan reads and
    // parses every conversation file the adapter has. Without this it
    // refetched on every window focus, and each refetch rebuilds the row
    // list -- which unmounts any row with a copy in flight, and TanStack
    // drops a per-call callback whose component has gone. That is what made
    // a failed import able to say nothing at all.
    staleTime: IMPORT_SCAN_STALE_MS,
  })
}

/**
 * Whether/how "Continue externally" can work for ONE external session.
 *
 * `enabled` is the whole point of this hook and callers must treat it as
 * mandatory, not optional. The doc comment here used to claim the probe was
 * "queried lazily (only once a session row actually renders that action)" --
 * it was not. Every row mounted this unconditionally the moment it rendered,
 * so opening Claude Code with 400 saved chats fired 400 probes at once, each
 * of which opens and reads a file. That is what made the surface unusable at
 * real scale, and the comment describing the intent is why it went unnoticed.
 *
 * A row must never call this. The list renders hundreds of rows; only the ONE
 * chat whose details are open asks, which is why `ImportPicker` calls it once
 * for the expanded chat and nowhere else.
 */
export function useAgentImportContinuation(
  adapterId: string | null,
  externalSessionId: string | null
) {
  return useQuery({
    queryKey: keys.agentImportContinuation(adapterId ?? 'none', externalSessionId ?? 'none'),
    enabled: adapterId != null && externalSessionId != null,
    queryFn: async () =>
      unwrap(await commands.agentImportContinuationCapability(adapterId!, externalSessionId!)),
    // Whether a client can resume a given chat does not change while a window
    // is open, and re-probing on focus re-reads the file for no new answer.
    staleTime: IMPORT_SCAN_STALE_MS,
  })
}

/**
 * Bring in many chats with one press.
 *
 * One command rather than N mutations: the backend detects the client once for
 * the whole batch instead of per chat, and a single in-flight request is what
 * lets the action bar show one honest pending state. Firing 200 single-chat
 * mutations would also mean 200 cache invalidations and 200 toasts.
 *
 * The failure handler lives here, not at the call site, for the reason the
 * single-chat import already documents: a per-call callback is dropped if its
 * component unmounts before the mutation settles, and a long batch is exactly
 * when that happens.
 */
export function useImportExternalSessionBatch() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: async ({
      adapterId,
      externalSessionIds,
    }: {
      adapterId: string
      externalSessionIds: string[]
    }) => unwrap(await commands.agentImportSessionBatch(adapterId, externalSessionIds)),
    onError: (error, variables) => {
      log.error(
        `batch import of ${variables.externalSessionIds.length} chats failed: ${describeError(error)}`
      )
      toast.error('Those chats could not be brought in.', {
        description: 'Nothing was changed. You can try again.',
      })
    },
    onSuccess: (result, variables) => {
      qc.invalidateQueries({ queryKey: keys.agentImportScan(variables.adapterId) })
      qc.invalidateQueries({ queryKey: keys.agentSessionsAll })
      if (result.kind !== 'completed') return
      // Each chat that changed may be open in the conversation pane. Same
      // reasoning as the single import: telling someone messages arrived while
      // the transcript on screen still shows the old ones is worse than saying
      // nothing.
      for (const item of result.items) {
        const sessionId = importedSessionId(item.outcome)
        if (sessionId) qc.invalidateQueries({ queryKey: keys.agentSession(sessionId) })
      }
    },
  })
}

/**
 * Which tools are set to keep checking themselves for new chats.
 *
 * A failed read is NOT the same as "nothing is synced", so this stays a query
 * with its own error state rather than defaulting to an empty list -- the
 * caller has to be able to tell the difference, the same rule the scan list
 * follows.
 */
export function useAgentImportSyncPreferences() {
  return useQuery({
    queryKey: keys.agentImportSyncPreferences,
    queryFn: async () => unwrap(await commands.agentImportGetSyncPreferences()),
  })
}

/**
 * Turn keep-in-sync on or off for one tool.
 *
 * A refused write reports and leaves the toggle where it was. A switch that
 * silently springs back is the worst outcome here: the person believes GitWyrm
 * is watching a folder it is not, or the reverse.
 */
export function useSetImportSyncPreference() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: async ({ adapterId, enabled }: { adapterId: string; enabled: boolean }) =>
      unwrap(await commands.agentImportSetSyncPreference(adapterId, enabled)),
    onError: (error, { adapterId }) => {
      log.error(`could not save keep-in-sync for ${adapterId}: ${describeError(error)}`)
      toast.error('That choice could not be saved.', {
        description: 'Keep in sync is unchanged. You can try again.',
      })
    },
    onSuccess: (result) => {
      qc.invalidateQueries({ queryKey: keys.agentImportSyncPreferences })
      if (result.kind === 'failed') {
        log.error(`keep-in-sync choice was refused: ${result.detail}`)
        toast.error('That choice could not be saved.', {
          description: 'Keep in sync is unchanged. You can try again.',
        })
      }
    },
  })
}

/** Import (or incrementally refresh) one external session into a durable
 * GitWyrm session. Invalidates the adapter's scan (so `alreadyImported`
 * flips), the native session list (the new/updated session must appear there
 * immediately, per Rule #1: every action needs a visible result), and the
 * imported session's OWN query.
 *
 * That last one was missing, and it is the one that matters most on a
 * refresh: pressing Refresh on a chat already open in the conversation pane
 * appended the newly-found messages, said "Added 4 new messages", and left
 * the pane showing the transcript from before. The person was told messages
 * had arrived and could see none of them.
 *
 * Both siblings below already do this; import was the only one that did not,
 * and the only one that can add many messages at once. */
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
    // Here rather than at the call site. A per-call `onError` is dropped if
    // the component that passed it unmounts before the mutation settles, and
    // the row this runs from is rebuilt whenever the scan refetches -- so the
    // one place a failure was reported was also the place most likely to be
    // gone when it happened. A hook-level handler always runs.
    onError: (error, variables) => {
      log.error(
        `import session failed for ${variables.externalSessionId}: ${describeError(error)}`
      )
      toast.error('That chat could not be brought in.', {
        description: 'Nothing was changed. You can try again.',
      })
    },
    onSuccess: (result, variables) => {
      qc.invalidateQueries({ queryKey: keys.agentImportScan(variables.adapterId) })
      qc.invalidateQueries({ queryKey: keys.agentSessionsAll })
      const sessionId = importedSessionId(result)
      if (sessionId) {
        qc.invalidateQueries({ queryKey: keys.agentSession(sessionId) })
      }
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
    onError: (error, sessionId) => {
      log.error(`continue imported session here failed for ${sessionId}: ${describeError(error)}`)
      toast.error('That chat could not be continued here.', {
        description: 'Nothing was changed. You can try again.',
      })
    },
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
    onError: (error, { sessionId }) => {
      log.error(`unlink imported session failed for ${sessionId}: ${describeError(error)}`)
      toast.error('That chat could not be unlinked.', {
        description: 'Nothing was changed. You can try again.',
      })
    },
    onSuccess: (_result, { sessionId, adapterId, externalSessionId }) => {
      qc.invalidateQueries({ queryKey: keys.agentSession(sessionId) })
      qc.invalidateQueries({ queryKey: keys.agentSessionsAll })
      qc.invalidateQueries({ queryKey: keys.agentImportScan(adapterId) })
      qc.invalidateQueries({ queryKey: keys.agentImportContinuation(adapterId, externalSessionId) })
    },
  })
}
