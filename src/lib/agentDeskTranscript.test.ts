import { describe, expect, it } from 'vitest'
import type { SessionMessage } from '@/lib/bindings'
import { foldThoughtSummaries } from './agentDeskTranscript'

function msg(id: string, kind: SessionMessage['kind']): SessionMessage {
  return {
    messageId: id,
    segmentId: 'seg-1',
    role: kind === 'user' ? 'user' : 'assistant',
    timestamp: '2026-08-19T00:00:00Z',
    plainContent: id,
    renderedContent: null,
    provider: null,
    model: null,
    kind,
    executionId: 'exec-1',
    sequence: 1,
    import: null,
    targets: [],
  }
}

describe('foldThoughtSummaries', () => {
  it('folds a thought into the assistant reply that immediately follows it', () => {
    const thought = msg('t1', 'thoughtSummary')
    const reply = msg('r1', 'assistant')
    const { thoughtFor, folded } = foldThoughtSummaries([thought, reply])
    expect(thoughtFor.get('r1')).toBe(thought)
    expect(folded.has('t1')).toBe(true)
    expect(thoughtFor.has('t1')).toBe(false)
  })

  it('folds a thought into a result message that immediately follows it', () => {
    const thought = msg('t1', 'thoughtSummary')
    const result = msg('res1', 'result')
    const { thoughtFor, folded } = foldThoughtSummaries([thought, result])
    expect(thoughtFor.get('res1')).toBe(thought)
    expect(folded.has('t1')).toBe(true)
  })

  it('renders a thought standalone when it is the last message in the transcript', () => {
    const thought = msg('t1', 'thoughtSummary')
    const { thoughtFor, folded } = foldThoughtSummaries([msg('u1', 'user'), thought])
    expect(thoughtFor.get('t1')).toBe(thought)
    expect(folded.has('t1')).toBe(false)
  })

  it('renders a thought standalone when followed by an unrelated kind', () => {
    const thought = msg('t1', 'thoughtSummary')
    const { thoughtFor, folded } = foldThoughtSummaries([thought, msg('tool1', 'tool')])
    expect(thoughtFor.get('t1')).toBe(thought)
    expect(folded.has('t1')).toBe(false)
  })

  it('handles a transcript with no thought summaries', () => {
    const { thoughtFor, folded } = foldThoughtSummaries([msg('u1', 'user'), msg('a1', 'assistant')])
    expect(thoughtFor.size).toBe(0)
    expect(folded.size).toBe(0)
  })

  it('keeps two consecutive thoughts distinct: the first stays standalone, the second folds', () => {
    const t1 = msg('t1', 'thoughtSummary')
    const t2 = msg('t2', 'thoughtSummary')
    const reply = msg('r1', 'assistant')
    const { thoughtFor, folded } = foldThoughtSummaries([t1, t2, reply])
    expect(thoughtFor.get('t1')).toBe(t1)
    expect(folded.has('t1')).toBe(false)
    expect(thoughtFor.get('r1')).toBe(t2)
    expect(folded.has('t2')).toBe(true)
  })
})
