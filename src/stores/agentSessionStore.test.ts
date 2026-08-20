import { describe, it, expect, beforeEach } from 'vitest'
import type { AgentSessionEvent, AgentSession, SessionMessage } from '@/lib/bindings'
import { mergeSessionMessages, useAgentSessionStore } from './agentSessionStore'

const SESSION = 'session-1'
const EXEC = 'exec-1'

beforeEach(() => {
  useAgentSessionStore.setState({ bySession: {} })
})

function message(id: string): SessionMessage {
  return {
    messageId: id,
    segmentId: 'seg-1',
    role: 'assistant',
    timestamp: '2026-08-19T00:00:00Z',
    plainContent: `content-${id}`,
    renderedContent: null,
    provider: null,
    model: null,
    kind: 'assistant',
    executionId: EXEC,
    sequence: null,
    import: null,
    targets: [],
  }
}

function appended(sequence: number, id: string, executionId: string | null = EXEC): AgentSessionEvent {
  return {
    sessionId: SESSION,
    executionId,
    sequence,
    occurredAt: '2026-08-19T00:00:00Z',
    kind: { kind: 'messageAppended', message: message(id) },
  }
}

describe('agentSessionStore', () => {
  it('applies a new event and appends the message', () => {
    useAgentSessionStore.getState().applyEvent(appended(1, 'm1'))
    const entry = useAgentSessionStore.getState().bySession[SESSION]
    expect(entry?.messages.map((m) => m.messageId)).toEqual(['m1'])
  })

  /**
   * Duplicate event: the same sequence delivered twice (e.g. a reconnect
   * replaying from before the last-seen point) must not double the message.
   */
  it('ignores a duplicate sequence for the same execution', () => {
    useAgentSessionStore.getState().applyEvent(appended(1, 'm1'))
    useAgentSessionStore.getState().applyEvent(appended(1, 'm1'))
    const entry = useAgentSessionStore.getState().bySession[SESSION]
    expect(entry?.messages).toHaveLength(1)
  })

  /**
   * Stale event: sequence older than the highest already seen for that
   * execution (out-of-order delivery) is dropped, matching the backend's own
   * idempotency rule in `design.md`'s "Failure behavior" section.
   */
  it('ignores a stale (out-of-order) sequence', () => {
    useAgentSessionStore.getState().applyEvent(appended(5, 'm5'))
    useAgentSessionStore.getState().applyEvent(appended(2, 'm2'))
    const entry = useAgentSessionStore.getState().bySession[SESSION]
    expect(entry?.messages.map((m) => m.messageId)).toEqual(['m5'])
  })

  /**
   * A gap in sequence numbers (1, then 3) is not itself invalid here -- the
   * store's job is only to reject duplicates/staleness, not to demand
   * contiguity. Both messages land; detecting the gap for diagnostics is a
   * backend concern per `design.md`.
   */
  it('accepts a sequence gap without losing either message', () => {
    useAgentSessionStore.getState().applyEvent(appended(1, 'm1'))
    useAgentSessionStore.getState().applyEvent(appended(3, 'm3'))
    const entry = useAgentSessionStore.getState().bySession[SESSION]
    expect(entry?.messages.map((m) => m.messageId)).toEqual(['m1', 'm3'])
  })

  /**
   * `executionSuperseded` is forward-facing: the backend does not emit it
   * today (`commands/airun.rs::route_to_agent_desk` drops
   * `BridgeOutcome::ExecutionSuperseded` silently, per design.md's "Event
   * for replaced execution: ignore it in both backend and frontend"), so
   * this event is not currently reachable from a live run. It is exercised
   * here directly (constructing the event by hand, not via a fixture that
   * mimics what the backend sends today) so the handling is already correct
   * whenever a future change decides to start emitting one.
   */
  it('drops further events for an execution once it is superseded (forward-facing: not yet emitted by the backend)', () => {
    const s = useAgentSessionStore.getState()
    s.applyEvent(appended(1, 'm1'))
    s.applyEvent({
      sessionId: SESSION,
      executionId: EXEC,
      sequence: 2,
      occurredAt: '2026-08-19T00:00:00Z',
      kind: { kind: 'executionSuperseded', executionId: EXEC },
    })
    // A late event from the now-superseded execution.
    s.applyEvent(appended(2, 'm2-late'))
    const entry = useAgentSessionStore.getState().bySession[SESSION]
    expect(entry?.messages.map((m) => m.messageId)).toEqual(['m1'])
  })

  it('tracks state changes independently per session', () => {
    useAgentSessionStore.getState().applyEvent({
      sessionId: SESSION,
      executionId: EXEC,
      sequence: 1,
      occurredAt: '2026-08-19T00:00:00Z',
      kind: { kind: 'stateChanged', state: 'working' },
    })
    expect(useAgentSessionStore.getState().bySession[SESSION]?.state).toBe('working')
  })

  /**
   * "Two windows" scenario: both the main window's listener and the Desk's
   * listener apply the same broadcast event to this shared module-level
   * store. The dedupe-by-messageId path (not just by sequence) is what keeps
   * that from ever doubling a message even if two calls interleave.
   */
  it('is idempotent when the same event is applied from two listeners', () => {
    const event = appended(1, 'm1')
    useAgentSessionStore.getState().applyEvent(event)
    useAgentSessionStore.getState().applyEvent(event)
    const entry = useAgentSessionStore.getState().bySession[SESSION]
    expect(entry?.messages).toHaveLength(1)
  })
})

describe('mergeSessionMessages', () => {
  function session(messages: SessionMessage[]): AgentSession {
    return {
      header: {
        schemaVersion: 1,
        sessionId: SESSION,
        repoId: 'repo-1',
        repoPath: 'C:/repo',
        repoName: 'repo',
        title: 'Test',
        source: { kind: 'manual', repoId: 'repo-1' },
        intent: 'ask',
        state: 'ready',
        createdAt: '2026-08-19T00:00:00Z',
        updatedAt: '2026-08-19T00:00:00Z',
        unread: false,
        changedFileCount: 0,
        activeExecutionId: null,
        archived: false,
      },
      segments: [],
      messages,
      executions: [],
      attachments: [],
    }
  }

  /**
   * "Restart hydration" scenario: after a relaunch there is no live overlay
   * yet, only the query's persisted messages -- those must render as-is.
   */
  it('renders persisted messages alone when there is no live overlay', () => {
    const s = session([message('m1'), message('m2')])
    expect(mergeSessionMessages(s, []).map((m) => m.messageId)).toEqual(['m1', 'm2'])
  })

  it('appends live messages not yet present in the persisted session', () => {
    const s = session([message('m1')])
    const merged = mergeSessionMessages(s, [message('m2')])
    expect(merged.map((m) => m.messageId)).toEqual(['m1', 'm2'])
  })

  /**
   * Task 5.4: after a query refetch pulls a live message into `session.messages`,
   * the same message must not also be rendered from the live overlay.
   */
  it('does not duplicate a live message once the query has persisted it', () => {
    const s = session([message('m1'), message('m2')])
    const merged = mergeSessionMessages(s, [message('m2')])
    expect(merged.map((m) => m.messageId)).toEqual(['m1', 'm2'])
  })

  it('returns the live messages as-is when there is no session yet', () => {
    expect(mergeSessionMessages(null, [message('m1')]).map((m) => m.messageId)).toEqual(['m1'])
  })
})
