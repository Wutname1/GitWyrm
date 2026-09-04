import { create } from 'zustand'
import type { AgentSession, AgentSessionEvent, SessionMessage, SessionState } from '@/lib/bindings'

/**
 * Live event overlay for Agent Desk sessions, keyed by session ID.
 *
 * Canonical content lives in the backend's per-session JSON files; queries
 * (`useAgentSession`) load it. This store only holds what has arrived over
 * `agent-session-event` since the query last resolved, keyed by session and
 * sequence so a duplicate or late/superseded event is a no-op rather than a
 * second copy in the transcript -- the same rule `design.md`'s "Failure
 * behavior" section states for the backend, applied again on the frontend
 * because a query refresh and a live event can each deliver the same message.
 *
 * Deliberately not the database: `useAgentSession` merges this overlay with
 * the query's persisted messages by `messageId`, so restarting the app (no
 * live events yet) and staying open for hours (many live events) both render
 * the same transcript.
 */
interface SessionLiveEntry {
  /** Live-appended messages not guaranteed to be in the last query result yet. */
  messages: SessionMessage[]
  /** Highest sequence number seen per execution, for gap/duplicate detection. */
  lastSequenceByExecution: Record<string, number>
  /**
   * Executions where an event's sequence jumped past `lastSeen + 1`, meaning
   * at least one event never arrived.
   *
   * The backend deliberately leaves this half to the client: it persists a
   * gapped event with its sequence un-renumbered precisely "so a frontend
   * listener can notice the jump" (`bridge.rs`). Nothing noticed. A dropped
   * event was silently accepted and the transcript rendered as complete with
   * turns missing -- which is the one thing this product says it will never
   * do, in the surface a person reads to decide whether to keep the work.
   */
  gappedExecutionIds: string[]
  /** Latest state pushed by a `stateChanged` event, if newer than the query. */
  state: SessionState | null
  /** Executions superseded mid-flight; their further events are dropped. */
  supersededExecutionIds: string[]
}

const EMPTY_ENTRY: SessionLiveEntry = {
  messages: [],
  lastSequenceByExecution: {},
  gappedExecutionIds: [],
  state: null,
  supersededExecutionIds: [],
}

interface AgentSessionStore {
  bySession: Record<string, SessionLiveEntry>
  /** Applies one durable event, ignoring duplicates and superseded executions. */
  applyEvent: (event: AgentSessionEvent) => void
  /**
   * Drops the live overlay for a session once its messages are known to be
   * folded into a query result, so the overlay never grows without bound
   * across a long-lived session.
   */
  clearSession: (sessionId: string) => void
}

export const useAgentSessionStore = create<AgentSessionStore>((set) => ({
  bySession: {},
  applyEvent: (event) =>
    set((s) => {
      const entry = s.bySession[event.sessionId] ?? EMPTY_ENTRY
      const payload = event.kind

      // An event tied to an execution this session has already moved past.
      //
      // Forward-facing: per design.md ("Event for replaced execution:
      // ignore it in both backend and frontend"), the backend today never
      // emits `executionSuperseded` -- `route_to_agent_desk` in
      // `commands/airun.rs` drops `BridgeOutcome::ExecutionSuperseded`
      // silently rather than turning it into an `agent-session-event`,
      // since nothing was persisted for such an event to describe (see
      // "Persist an event before emitting it to the UI", also design.md).
      // This branch, and the dedicated test for it in
      // `agentSessionStore.test.ts`, exist so a listener is already correct
      // on the day the backend does start emitting one (e.g. to let an
      // optimistic UI reconcile) -- not because it can arrive today.
      if (payload.kind === 'executionSuperseded') {
        return {
          bySession: {
            ...s.bySession,
            [event.sessionId]: {
              ...entry,
              supersededExecutionIds: [
                ...entry.supersededExecutionIds,
                payload.executionId,
              ],
            },
          },
        }
      }

      if (event.executionId && entry.supersededExecutionIds.includes(event.executionId)) {
        return s
      }

      if (payload.kind === 'stateChanged') {
        return {
          bySession: {
            ...s.bySession,
            [event.sessionId]: { ...entry, state: payload.state },
          },
        }
      }

      // messageUpdated: an existing message's content was replaced in place
      // (backend coalescing of consecutive streamed-text steps within one
      // execution -- see `agentdesk::bridge::map_run_step`'s doc comment).
      // This must NOT go through the messageAppended dedupe-by-messageId
      // check below: that check exists to drop an exact repeat, but an
      // update intentionally reuses the same `messageId` with new content,
      // so "already have this id" is exactly the case this branch needs to
      // handle by replacing, not the case to drop. Sequence still only
      // advances (never goes backward), matching messageAppended's own rule,
      // since an update still consumes a sequence number on the backend.
      if (payload.kind === 'messageUpdated') {
        const key = event.executionId ?? '__none__'
        const seenSoFar = entry.lastSequenceByExecution[key] ?? 0
        if (event.sequence !== 0 && event.sequence <= seenSoFar) {
          return s
        }
        const targetId = payload.message.messageId
        const index = entry.messages.findIndex((m) => m.messageId === targetId)
        const messages =
          index === -1
            ? [...entry.messages, payload.message]
            : entry.messages.map((m, i) => (i === index ? payload.message : m))
        return {
          bySession: {
            ...s.bySession,
            [event.sessionId]: {
              ...entry,
              messages,
              lastSequenceByExecution: {
                ...entry.lastSequenceByExecution,
                [key]: Math.max(seenSoFar, event.sequence),
              },
            },
          },
        }
      }

      // messageAppended: dedupe by sequence per execution. `sequence` is
      // monotonic per execution ID (see the `AgentSessionEvent` doc comment
      // in bindings.ts); a duplicate or out-of-order-old sequence is ignored,
      // matching the backend's own idempotency rule so a reconnect or a
      // second listener never doubles a message.
      const execKey = event.executionId ?? '__none__'
      const lastSeen = entry.lastSequenceByExecution[execKey] ?? 0
      if (event.sequence !== 0 && event.sequence <= lastSeen) {
        return s
      }
      // Already applied this exact message (e.g. delivered to two listeners
      // in the same window before either could dedupe by sequence alone).
      if (entry.messages.some((m) => m.messageId === payload.message.messageId)) {
        return s
      }

      // A sequence that skips past the next expected number means an event
      // never arrived. `sequence === 0` is the "not from an execution" case
      // and is not part of any run, and `lastSeen === 0` is the first event
      // this window has seen for the execution -- neither is a gap.
      const gapped =
        event.sequence !== 0 && lastSeen !== 0 && event.sequence > lastSeen + 1
          ? entry.gappedExecutionIds.includes(execKey)
            ? entry.gappedExecutionIds
            : [...entry.gappedExecutionIds, execKey]
          : entry.gappedExecutionIds

      return {
        bySession: {
          ...s.bySession,
          [event.sessionId]: {
            ...entry,
            messages: [...entry.messages, payload.message],
            gappedExecutionIds: gapped,
            lastSequenceByExecution: {
              ...entry.lastSequenceByExecution,
              [execKey]: Math.max(lastSeen, event.sequence),
            },
          },
        },
      }
    }),
  clearSession: (sessionId) =>
    set((s) => {
      if (!(sessionId in s.bySession)) return s
      const next = { ...s.bySession }
      delete next[sessionId]
      return { bySession: next }
    }),
}))

/** Live messages for one session, oldest first, empty when none have arrived. */
export function selectLiveMessages(sessionId: string | null): SessionMessage[] {
  if (!sessionId) return []
  return useAgentSessionStore.getState().bySession[sessionId]?.messages ?? []
}

/** The most recent state a live event reported, or null if none has arrived. */
export function selectLiveState(sessionId: string | null): SessionState | null {
  if (!sessionId) return null
  return useAgentSessionStore.getState().bySession[sessionId]?.state ?? null
}

/**
 * Merges a query's persisted session with this store's live overlay into the
 * transcript a component should render.
 *
 * A message id present on both sides is resolved by `sequence`, not by
 * "persisted always wins": persist-before-emit (design.md) means a live
 * event's content is never older than what was on disk when it was emitted,
 * but the *query* result being merged against can itself predate that emit
 * (a refetch that raced the write, or one that simply has not happened
 * since). Backend coalescing (`agentdesk::bridge`'s `MessageUpdated`, for
 * streamed-text chunks folded into one growing message) makes this matter in
 * practice: without a sequence comparison, a stale persisted copy of a
 * message id would permanently shadow a newer live update to that same id,
 * since the old "persisted wins by identity" rule filtered out any live
 * message whose id merely already existed. Comparing `sequence` (`None`
 * treated as older than any real sequence, since it means the message
 * predates sequencing entirely) picks whichever side actually has the newest
 * content, and a message id absent from `session.messages` is always new by
 * definition.
 */
/**
 * Whether every live message is already folded into the persisted session, so
 * the overlay is pure duplication and can be dropped.
 *
 * `clearSession` was written to stop the overlay growing without bound and
 * then never called, because nothing decided WHEN it was safe -- so a long
 * session kept every live message in two places for the life of the window,
 * and `mergeSessionMessages` re-walked both arrays on every render.
 *
 * Safe means: the persisted copy exists AND is at least as new. A live
 * message the query has not caught up with yet must survive, or the
 * transcript would lose a message it had already shown.
 */
export function liveOverlayIsRedundant(
  session: AgentSession | null | undefined,
  liveMessages: SessionMessage[]
): boolean {
  if (!session || liveMessages.length === 0) return false
  const persisted = new Map(session.messages.map((m) => [m.messageId, m.sequence ?? 0]))
  return liveMessages.every((live) => {
    const seq = persisted.get(live.messageId)
    return seq !== undefined && seq >= (live.sequence ?? 0)
  })
}

export function mergeSessionMessages(
  session: AgentSession | null | undefined,
  liveMessages: SessionMessage[]
): SessionMessage[] {
  if (!session) return liveMessages
  if (liveMessages.length === 0) return session.messages

  const persistedIndexById = new Map(session.messages.map((m, i) => [m.messageId, i]))
  const merged = [...session.messages]
  const appended: SessionMessage[] = []

  for (const live of liveMessages) {
    const at = persistedIndexById.get(live.messageId)
    if (at === undefined) {
      appended.push(live)
      continue
    }
    if ((live.sequence ?? 0) > (merged[at].sequence ?? 0)) {
      merged[at] = live
    }
  }

  return appended.length === 0 ? merged : [...merged, ...appended]
}
