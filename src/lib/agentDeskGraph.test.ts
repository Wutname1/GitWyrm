import { describe, expect, it } from 'vitest'
import { sessionHasGraph } from './agentDeskGraph'

type Session = Parameters<typeof sessionHasGraph>[0]

function session(overrides: {
  graphStartedAt?: string | null
  executions?: Array<{ parentExecutionId: string | null; proposedGraph: unknown }>
}): Session {
  return {
    header: { graphStartedAt: overrides.graphStartedAt ?? null },
    executions: (overrides.executions ?? []).map((e) => ({ ...e })),
  } as unknown as Session
}

describe('sessionHasGraph', () => {
  it('is false for no session and for a solo chat', () => {
    expect(sessionHasGraph(null)).toBe(false)
    expect(sessionHasGraph(undefined)).toBe(false)
    expect(sessionHasGraph(session({}))).toBe(false)
    // One lead execution with no helpers and no proposal is still solo.
    expect(sessionHasGraph(session({ executions: [{ parentExecutionId: null, proposedGraph: null }] }))).toBe(false)
  })

  it('is true once a helper exists', () => {
    expect(
      sessionHasGraph(
        session({
          executions: [
            { parentExecutionId: null, proposedGraph: null },
            { parentExecutionId: 'lead-1', proposedGraph: null },
          ],
        })
      )
    ).toBe(true)
  })

  it('is true while a proposed team waits for Start', () => {
    // The Start button lives in the graph panel, so hiding the panel here
    // would hide the only way to start the team.
    expect(sessionHasGraph(session({ executions: [{ parentExecutionId: null, proposedGraph: { helpers: [] } }] }))).toBe(true)
  })

  it('is true once the graph has started even if helper records are not loaded yet', () => {
    expect(sessionHasGraph(session({ graphStartedAt: '2026-09-02T00:00:00Z' }))).toBe(true)
  })
})
