import { describe, expect, it } from 'vitest'
import { canViewNodeChanges, latestActivityLine, resultForNode, sessionHasGraph } from './agentDeskGraph'

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

describe('latestActivityLine', () => {
  const msg = (
    executionId: string | null,
    sequence: number | null,
    role: 'user' | 'assistant' | 'system',
    kind: 'user' | 'assistant' | 'tool' | 'result' | 'system' | 'thoughtSummary' | 'approval',
    plainContent: string
  ) => ({ executionId, sequence, role, kind, plainContent })

  it('is null when the execution has said nothing yet', () => {
    expect(latestActivityLine([], 'h1')).toBeNull()
    expect(latestActivityLine([msg('other', 1, 'assistant', 'tool', 'Read a file')], 'h1')).toBeNull()
  })

  it('picks the highest sequence, not the last array position, and keeps only the first line', () => {
    const messages = [
      msg('h1', 2, 'assistant', 'tool', 'Ran the tests: 3 passed\nsecond line'),
      msg('h1', 1, 'assistant', 'tool', 'Read src/lib.rs'),
    ]
    expect(latestActivityLine(messages, 'h1')).toBe('Ran the tests: 3 passed')
  })

  it('ignores what the person typed and the final result', () => {
    const messages = [
      msg('h1', 1, 'assistant', 'assistant', '  Looking at the crash log  '),
      msg('h1', 2, 'assistant', 'result', 'All done'),
      msg('h1', 9, 'user', 'user', 'please hurry'),
    ]
    expect(latestActivityLine(messages, 'h1')).toBe('Looking at the crash log')
  })

  it('falls back to transcript order when messages carry no sequence', () => {
    const messages = [msg('h1', null, 'system', 'system', 'Getting ready'), msg('h1', null, 'assistant', 'tool', 'Editing a file')]
    expect(latestActivityLine(messages, 'h1')).toBe('Editing a file')
  })
})

describe('result helpers', () => {
  const record = (executionId: string, worktreePath: string | null, changed: number) =>
    ({ executionId, worktreePath, changedPaths: Array.from({ length: changed }, (_, i) => ({ path: `f${i}` })) }) as unknown as import('@/lib/bindings').ResultRecord

  it('finds a node result only once one exists', () => {
    expect(resultForNode(undefined, 'h1')).toBeNull()
    expect(resultForNode([record('h2', null, 0)], 'h1')).toBeNull()
    expect(resultForNode([record('h1', null, 0)], 'h1')?.executionId).toBe('h1')
  })

  it('offers View changes only for a worktree result with changed files', () => {
    expect(canViewNodeChanges(null)).toBe(false)
    expect(canViewNodeChanges(record('h1', null, 2))).toBe(false)
    expect(canViewNodeChanges(record('h1', 'C:/wt/h1', 0))).toBe(false)
    expect(canViewNodeChanges(record('h1', 'C:/wt/h1', 2))).toBe(true)
  })
})
