import { describe, expect, it } from 'vitest'
import type { AgentSession, StartExecutionOutcome, StartGraphOutcome } from '@/lib/bindings'
import {
  invalidGraphReason,
  startFailureCardForError,
  startFailureCardForExecution,
  startFailureCardForGraph,
} from './agentDeskStartFailure'

// Only the `kind` matters to the mapping; the session payload is never read.
const session = {} as AgentSession

describe('startFailureCardForExecution', () => {
  it('shows no card when the run started', () => {
    expect(startFailureCardForExecution({ kind: 'started', session, execution_id: 'e1' })).toBeNull()
  })

  it('treats alreadyRunning as not a failure (a queued follow-up starts the next turn)', () => {
    expect(startFailureCardForExecution({ kind: 'alreadyRunning', execution_id: 'e1' })).toBeNull()
  })

  it('keeps notFound and damaged as toasts: no retry can fix a missing chat', () => {
    expect(startFailureCardForExecution({ kind: 'notFound' })).toBeNull()
    expect(startFailureCardForExecution({ kind: 'damaged', reason: 'bad json' })).toBeNull()
  })

  it('sourceMissing offers Open project then Try again', () => {
    const card = startFailureCardForExecution({ kind: 'sourceMissing', detail: 'C:/repo is not open' })
    expect(card).not.toBeNull()
    expect(card!.title).toBe('This chat needs its project open')
    expect(card!.actions).toEqual(['openProject', 'tryAgain'])
    expect(card!.detail).toBe('C:/repo is not open')
  })

  it('adapterUnsupported offers another tool and Try again', () => {
    const card = startFailureCardForExecution({ kind: 'adapterUnsupported', detail: 'copilot too old' })
    expect(card!.actions).toEqual(['pickProvider', 'tryAgain'])
    expect(card!.detail).toBe('copilot too old')
    expect(card!.title).toMatch(/cannot run this chat/)
  })

  it('providerReconnect says to sign in again and offers another tool', () => {
    const card = startFailureCardForExecution({ kind: 'providerReconnect', detail: 'token expired' })
    expect(card!.title).toMatch(/sign in again/)
    expect(card!.actions).toEqual(['pickProvider', 'tryAgain'])
  })

  it('unsupportedProvider names the tool that was asked for', () => {
    const card = startFailureCardForExecution({ kind: 'unsupportedProvider', requested: 'gemini' })
    expect(card!.title).toContain('gemini')
    expect(card!.actions).toContain('pickProvider')
  })

  it('worktreeFailed, writeFailed and unavailable show the detail with Try again only', () => {
    for (const outcome of [
      { kind: 'worktreeFailed', detail: 'disk full' },
      { kind: 'writeFailed', detail: 'disk full' },
      { kind: 'unavailable', detail: 'disk full' },
    ] as StartExecutionOutcome[]) {
      const card = startFailureCardForExecution(outcome)
      expect(card!.kind).toBe(outcome.kind)
      expect(card!.actions).toEqual(['tryAgain'])
      expect(card!.detail).toBe('disk full')
    }
  })

  it('an unknown kind still gets a card with Try again and the kind as detail', () => {
    const card = startFailureCardForExecution({ kind: 'somethingNew' } as unknown as StartExecutionOutcome)
    expect(card!.actions).toEqual(['tryAgain'])
    expect(card!.detail).toBe('somethingNew')
  })

  it('every card reads simply: short title, no jargon tokens, and at least one action', () => {
    const outcomes: StartExecutionOutcome[] = [
      { kind: 'sourceMissing', detail: 'x' },
      { kind: 'adapterUnsupported', detail: 'x' },
      { kind: 'providerReconnect', detail: 'x' },
      { kind: 'unsupportedProvider', requested: 'x' },
      { kind: 'worktreeFailed', detail: 'x' },
      { kind: 'writeFailed', detail: 'x' },
      { kind: 'unavailable', detail: 'x' },
    ]
    for (const outcome of outcomes) {
      const card = startFailureCardForExecution(outcome)!
      expect(card.title.length).toBeLessThan(70)
      expect(card.body).not.toMatch(/adapter|worktree|provider|execution|IPC/i)
      expect(card.actions.length).toBeGreaterThan(0)
      expect(card.actions).toContain('tryAgain')
    }
  })
})

describe('startFailureCardForGraph', () => {
  it('shows no card for started, stale, alreadyStarted, noProposal, notFound, damaged', () => {
    const quiet: StartGraphOutcome[] = [
      { kind: 'started', session, lead_execution_id: 'l', started_helpers: [] },
      { kind: 'stale', lead_execution_id: 'l', current_fingerprint: 'f' },
      { kind: 'alreadyStarted' },
      { kind: 'noProposal' },
      { kind: 'notFound' },
      { kind: 'damaged', reason: 'r' },
    ]
    for (const outcome of quiet) expect(startFailureCardForGraph(outcome)).toBeNull()
  })

  it('sourceMissing offers Open project then Try again, same as the composer', () => {
    const card = startFailureCardForGraph({ kind: 'sourceMissing', detail: 'closed' })
    expect(card!.actions).toEqual(['openProject', 'tryAgain'])
  })

  it('invalid explains the reason in plain words', () => {
    const card = startFailureCardForGraph({ kind: 'invalid', reason: { kind: 'cycle' } as never })
    expect(card!.detail).toBe(invalidGraphReason('cycle'))
    expect(card!.actions).toEqual(['tryAgain'])
  })

  it('worktreeFailed names the helper', () => {
    const card = startFailureCardForGraph({ kind: 'worktreeFailed', node_id: 'h1', detail: 'no space' })
    expect(card!.title).toContain('h1')
    expect(card!.detail).toBe('no space')
    expect(card!.actions).toEqual(['tryAgain'])
  })

  it('writeFailed and unavailable get Try again', () => {
    expect(startFailureCardForGraph({ kind: 'writeFailed', detail: 'x' })!.actions).toEqual(['tryAgain'])
    expect(startFailureCardForGraph({ kind: 'unavailable', detail: 'x' })!.actions).toEqual(['tryAgain'])
  })
})

describe('startFailureCardForError', () => {
  it('turns a thrown error into a Try again card carrying the message', () => {
    const card = startFailureCardForError('IPC broke')
    expect(card.kind).toBe('error')
    expect(card.detail).toBe('IPC broke')
    expect(card.actions).toEqual(['tryAgain'])
  })
})

describe('invalidGraphReason', () => {
  it('has a sentence for every known reason and a fallback', () => {
    for (const reason of [
      'tooManyHelpers',
      'duplicateNodeId',
      'unknownDependency',
      'cycle',
      'emptyJob',
      'missingAllowedPaths',
    ]) {
      expect(invalidGraphReason(reason)).not.toBe('This plan is no longer valid.')
    }
    expect(invalidGraphReason('whatever')).toBe('This plan is no longer valid.')
  })
})
