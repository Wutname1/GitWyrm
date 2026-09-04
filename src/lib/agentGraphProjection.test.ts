import { describe, expect, it } from 'vitest'
import type { ExecutionRecord } from '@/lib/bindings'
import { buildGraphTree, graphSummary, isNodeActive, nodeDotTone, nodeStatusLabel } from './agentGraphProjection'
import { runIsActive } from './agentDeskResult'

function exec(overrides: Partial<ExecutionRecord> & Pick<ExecutionRecord, 'executionId' | 'state'>): ExecutionRecord {
  return {
    sessionId: 'sess-1',
    parentExecutionId: overrides.parentExecutionId ?? null,
    startedAt: '2026-01-01T00:00:00Z',
    endedAt: null,
    lastSequence: 0,
    jobTitle: overrides.jobTitle ?? null,
    jobDescription: overrides.jobDescription ?? null,
    helperRole: overrides.helperRole ?? null,
    allowedPaths: overrides.allowedPaths ?? [],
    worktreePath: overrides.worktreePath ?? null,
    branch: overrides.branch ?? null,
    dependsOn: overrides.dependsOn ?? [],
    changedFileCount: overrides.changedFileCount ?? 0,
    outputSummary: overrides.outputSummary ?? null,
    proposedGraph: overrides.proposedGraph ?? null,
    conflict: overrides.conflict ?? null,
    // Spread last so a field this helper does not list explicitly still
    // reaches the code under test. `reviewExecutionId` was silently dropped,
    // which would have made the review-turn tests below pass for the wrong
    // reason -- or, as it happened, fail for the right one.
    ...overrides,
  }
}

describe('buildGraphTree', () => {
  it('marks the lead node with no parent execution id', () => {
    const tree = buildGraphTree([exec({ executionId: 'lead', state: 'working' })])
    expect(tree[0].isLead).toBe(true)
  })

  it('marks a helper waiting on an unfinished dependency', () => {
    const tree = buildGraphTree([
      exec({ executionId: 'lead', state: 'working' }),
      exec({ executionId: 'h1', state: 'working', parentExecutionId: 'lead' }),
      exec({ executionId: 'h2', state: 'ready', parentExecutionId: 'lead', dependsOn: ['h1'] }),
    ])
    const h2 = tree.find((n) => n.execution.executionId === 'h2')!
    expect(h2.blockedOn).toEqual(['h1'])
  })

  it('a helper becomes unblocked once its dependency finishes', () => {
    const tree = buildGraphTree([
      exec({ executionId: 'lead', state: 'working' }),
      exec({ executionId: 'h1', state: 'finished', parentExecutionId: 'lead' }),
      exec({ executionId: 'h2', state: 'ready', parentExecutionId: 'lead', dependsOn: ['h1'] }),
    ])
    const h2 = tree.find((n) => n.execution.executionId === 'h2')!
    expect(h2.blockedOn).toEqual([])
  })

  it('a fourth ready helper is blocked once three are already active', () => {
    const tree = buildGraphTree([
      exec({ executionId: 'lead', state: 'working' }),
      exec({ executionId: 'h1', state: 'working', parentExecutionId: 'lead' }),
      exec({ executionId: 'h2', state: 'working', parentExecutionId: 'lead' }),
      exec({ executionId: 'h3', state: 'needsInput', parentExecutionId: 'lead' }),
      exec({ executionId: 'h4', state: 'ready', parentExecutionId: 'lead' }),
    ])
    const h4 = tree.find((n) => n.execution.executionId === 'h4')!
    expect(h4.blockedOn).toEqual([])
    expect(nodeStatusLabel(h4)).toBe('queued')
  })
})

describe('nodeStatusLabel', () => {
  it('reads waiting-at-a-gate as conflict when a conflict is set', () => {
    const node = {
      execution: exec({
        executionId: 'h1',
        state: 'needsInput',
        parentExecutionId: 'lead',
        conflict: {
          path: 'a.rs',
          conflictingWith: 'h2',
          baseText: 'base',
          helperText: 'a',
          integratedText: 'b',
        },
      }),
      isLead: false,
      blockedOn: [],
      waitingForSlot: false,
    }
    expect(nodeStatusLabel(node)).toBe('conflict')
  })

  it('reads plain needsInput as waiting', () => {
    const node = {
      execution: exec({ executionId: 'h1', state: 'needsInput' }),
      isLead: false,
      blockedOn: [],
      waitingForSlot: false,
    }
    expect(nodeStatusLabel(node)).toBe('waiting')
  })
})

describe('nodeDotTone', () => {
  it('the lead always gets the lead tone regardless of state', () => {
    const node = {
      execution: exec({ executionId: 'lead', state: 'needsInput' }),
      isLead: true,
      blockedOn: [],
      waitingForSlot: false,
    }
    expect(nodeDotTone(node)).toBe('lead')
  })

  it('a finished helper gets the done tone', () => {
    const node = {
      execution: exec({ executionId: 'h1', state: 'finished' }),
      isLead: false,
      blockedOn: [],
      waitingForSlot: false,
    }
    expect(nodeDotTone(node)).toBe('done')
  })

  it('a node that needs a person is never quieter than one that is fine', () => {
    // A failed or interrupted helper used to fall through to the neutral dot,
    // rendering CALMER than a working node (which at least pulses), so a
    // person scanning the tree could miss the only node that needed them.
    const helper = (state: ExecutionRecord['state']) => ({
      execution: exec({ executionId: 'h1', state }),
      isLead: false,
      blockedOn: [],
      waitingForSlot: false,
    })
    expect(nodeDotTone(helper('failed'))).toBe('attention')
    expect(nodeDotTone(helper('missingSource'))).toBe('attention')
    expect(nodeDotTone(helper('interrupted'))).toBe('interrupted')
  })
})

describe('the lead review turn is not a helper', () => {
  // The backend excludes it (`agentdesk::graph`: parent is Some AND not in
  // review_ids); the frontend filtered on "has a parent" alone. The review
  // record is created with the lead as its parent and starts `preparing`, so
  // for the whole of every review it showed as a fourth agent -- inflating
  // the header count and stealing a concurrency slot from a ready helper.
  const withReview = () => [
    exec({ executionId: 'lead', state: 'working', reviewExecutionId: 'review' }),
    exec({ executionId: 'h1', state: 'working', parentExecutionId: 'lead' }),
    exec({ executionId: 'h2', state: 'working', parentExecutionId: 'lead' }),
    exec({ executionId: 'review', state: 'preparing', parentExecutionId: 'lead' }),
  ]

  it('leaves the review turn out of the helper list', () => {
    const ids = buildGraphTree(withReview())
      .filter((n) => !n.isLead)
      .map((n) => n.execution.executionId)
    expect(ids).toEqual(['h1', 'h2'])
    expect(ids).not.toContain('review')
  })

  it('does not count the review turn as another working agent', () => {
    // Two helpers plus the lead are working; the reviewer is not a fourth.
    expect(graphSummary(withReview())).toBe('3 working')
  })

  it('still treats an ordinary helper as a helper', () => {
    const ids = buildGraphTree([
      exec({ executionId: 'lead', state: 'working' }),
      exec({ executionId: 'h1', state: 'working', parentExecutionId: 'lead' }),
    ])
      .filter((n) => !n.isLead)
      .map((n) => n.execution.executionId)
    expect(ids).toEqual(['h1'])
  })
})

describe('isNodeActive', () => {
  it('agrees with the one shared run predicate, by construction', () => {
    // This used to be a second definition of the same rule over the same
    // type. It agreed -- but the last time this rule lived in four
    // hand-written copies, one omitted `needsInput` and the transcript went
    // silent while an agent waited. Agreement that is not enforced is a
    // coincidence with a shelf life.
    for (const state of ['working', 'preparing', 'needsInput'] as const) {
      expect(isNodeActive(exec({ executionId: 'a', state }))).toBe(runIsActive(state))
    }
    for (const state of ['draft', 'finished', 'failed', 'stopped', 'interrupted'] as const) {
      expect(isNodeActive(exec({ executionId: 'a', state }))).toBe(runIsActive(state))
    }
  })

  it('working, preparing, and needsInput all count as active', () => {
    expect(isNodeActive(exec({ executionId: 'a', state: 'working' }))).toBe(true)
    expect(isNodeActive(exec({ executionId: 'a', state: 'preparing' }))).toBe(true)
    expect(isNodeActive(exec({ executionId: 'a', state: 'needsInput' }))).toBe(true)
    expect(isNodeActive(exec({ executionId: 'a', state: 'finished' }))).toBe(false)
  })

  it('a reconciled interrupted execution is not active', () => {
    expect(isNodeActive(exec({ executionId: 'a', state: 'interrupted' }))).toBe(false)
  })
})

describe('interrupted state (backend reconciliation on load)', () => {
  it('gets its own plain status label, distinct from failed/stopped', () => {
    const node = {
      execution: exec({ executionId: 'lead', state: 'interrupted' }),
      isLead: false,
      blockedOn: [],
      waitingForSlot: false,
    }
    const label = nodeStatusLabel(node)
    expect(label).not.toBe('failed')
    expect(label).not.toBe('stopped')
    expect(label).not.toBe('working')
    expect(label).not.toBe('done')
  })

  it('does not get the working or waiting dot tone, and is not silent either', () => {
    const node = {
      execution: exec({ executionId: 'h1', state: 'interrupted' }),
      isLead: false,
      blockedOn: [],
      waitingForSlot: false,
    }
    const tone = nodeDotTone(node)
    // The point of this test is that an interrupted agent must not be mistaken
    // for one that is still doing something. It used to assert `undefined`,
    // which satisfied that by making it the NEUTRAL dot -- calmer than a
    // working node, so a person scanning the tree could miss the one node that
    // needed them. Its own tone satisfies the original intent properly.
    expect(tone).not.toBe('working')
    expect(tone).not.toBe('waiting')
    expect(tone).not.toBe('done')
    expect(tone).toBe('interrupted')
  })

  it('is excluded from the working/waiting counts, but still visible in the summary', () => {
    // Excluded from working/waiting, as this test has always required -- but
    // not silent: a stopped agent that no peer is covering for has to reach
    // the header a person glances at, and the total stays so "1 stopped" does
    // not lose how many agents there were.
    const executions = [exec({ executionId: 'a', state: 'interrupted' }), exec({ executionId: 'b', state: 'finished' })]
    const summary = graphSummary(executions)
    expect(summary).not.toMatch(/working/)
    expect(summary).not.toMatch(/waiting/)
    expect(summary).toMatch(/2 agents/)
    expect(summary).toMatch(/1 stopped/)
  })
})

describe('graphSummary', () => {
  it('reports working and waiting counts together', () => {
    const executions = [
      exec({ executionId: 'a', state: 'working' }),
      exec({ executionId: 'b', state: 'needsInput' }),
      exec({ executionId: 'c', state: 'finished' }),
    ]
    expect(graphSummary(executions)).toBe('1 working · 1 waiting')
  })

  it('falls back to an agent count when nothing is working or waiting', () => {
    const executions = [exec({ executionId: 'a', state: 'finished' }), exec({ executionId: 'b', state: 'stopped' })]
    expect(graphSummary(executions)).toBe('2 agents')
  })
})
