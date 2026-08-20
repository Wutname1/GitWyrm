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
  /** Latest state pushed by a `stateChanged` event, if newer than the query. */
  state: SessionState | null
  /** Executions superseded mid-flight; their further events are dropped. */
  supersededExecutionIds: string[]
}

const EMPTY_ENTRY: SessionLiveEntry = {
  messages: [],
  lastSequenceByExecution: {},
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

      return {
        bySession: {
          ...s.bySession,
          [event.sessionId]: {
            ...entry,
            messages: [...entry.messages, payload.message],
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
 * Persisted messages win by identity: a live message already present in
 * `session.messages` (folded in by the backend, then returned by a refetch)
 * is not appended twice. Ordering follows `sequence` when both sides have
 * one, falling back to arrival order for messages that predate sequencing.
 */
export function mergeSessionMessages(
  session: AgentSession | null | undefined,
  liveMessages: SessionMessage[]
): SessionMessage[] {
  if (!session) return liveMessages
  const persistedIds = new Set(session.messages.map((m) => m.messageId))
  const extra = liveMessages.filter((m) => !persistedIds.has(m.messageId))
  if (extra.length === 0) return session.messages
  return [...session.messages, ...extra]
}
