import { describe, expect, it } from 'vitest'
import type { ExecutionRecord } from '@/lib/bindings'
import { buildGraphTree, graphSummary, isNodeActive, nodeDotTone, nodeStatusLabel } from './agentGraphProjection'

function exec(overrides: Partial<ExecutionRecord> & Pick<ExecutionRecord, 'executionId' | 'state'>): ExecutionRecord {
  return {
    executionId: overrides.executionId,
    sessionId: 'sess-1',
    parentExecutionId: overrides.parentExecutionId ?? null,
    state: overrides.state,
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
})

describe('isNodeActive', () => {
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

  it('does not get the working or waiting dot tone', () => {
    const node = {
      execution: exec({ executionId: 'h1', state: 'interrupted' }),
      isLead: false,
      blockedOn: [],
      waitingForSlot: false,
    }
    expect(nodeDotTone(node)).toBeUndefined()
  })

  it('is excluded from the working/waiting counts in the panel summary', () => {
    const executions = [exec({ executionId: 'a', state: 'interrupted' }), exec({ executionId: 'b', state: 'finished' })]
    expect(graphSummary(executions)).toBe('2 agents')
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
