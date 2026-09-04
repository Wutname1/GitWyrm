import { describe, it, expect, beforeEach } from 'vitest'
import type { RunEventKind, RunStep } from '@/lib/bindings'
import { useAiRunStore } from './aiRunStore'

const REPO = 'repo-1'
const SESSION = 'session-1'

beforeEach(() => {
  useAiRunStore.setState({ byRepo: {} })
})

function event(step: RunStep, summary: string, state: RunEventKind['state'] = 'working'): RunEventKind {
  return { repo_id: REPO, session_id: SESSION, state, summary, step }
}

const entry = () => useAiRunStore.getState().byRepo[REPO]

describe('bookkeeping steps stay out of the status line', () => {
  // `latest` is what the status bar tooltip shows as the run's current
  // activity. A step that only reports what a turn cost is not activity, and
  // letting one through makes the app say the agent is doing arithmetic.
  it('keeps a real activity summary', () => {
    useAiRunStore.getState().applyEvent(event({ kind: 'note', text: 'Reading the tests' }, 'Reading the tests'))
    expect(entry().latest).toBe('Reading the tests')
    expect(entry().steps).toHaveLength(1)
  })

  it('does not let usage overwrite it', () => {
    useAiRunStore.getState().applyEvent(event({ kind: 'note', text: 'Reading the tests' }, 'Reading the tests'))
    useAiRunStore
      .getState()
      .applyEvent(event({ kind: 'usage', usage: {} as never }, 'Used 15 tokens'))
    expect(entry().latest).toBe('Reading the tests')
  })

  it('does not let context usage overwrite it either', () => {
    // The same shape as `usage`, and it was not filtered -- so a real run's
    // status line read "Context window: 31000 of 200000" as though that were
    // what the agent was doing.
    useAiRunStore.getState().applyEvent(event({ kind: 'note', text: 'Reading the tests' }, 'Reading the tests'))
    useAiRunStore
      .getState()
      .applyEvent(
        event({ kind: 'contextUsage', used: 31000, size: 200000, cost_micro_usd: null }, 'Context window: 31000 of 200000')
      )
    expect(entry().latest).toBe('Reading the tests')
  })

  it('still records the run state from a bookkeeping step', () => {
    // Filtered from the stream, not ignored: the state it carries is real.
    useAiRunStore
      .getState()
      .applyEvent(event({ kind: 'contextUsage', used: 1, size: 2, cost_micro_usd: null }, 'x', 'needsYou'))
    expect(entry().state).toBe('needsYou')
  })
})
