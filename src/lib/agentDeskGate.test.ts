import { describe, expect, it } from 'vitest'
import type { GateRequest, SessionMessage } from '@/lib/bindings'
import { gateAnswerNoteFor, gateOptions, gateRequestOf, gateSummary } from '@/lib/agentDeskGate'

function approvalMessage(request: GateRequest): SessionMessage {
  return {
    messageId: 'm1',
    segmentId: 's1',
    role: 'assistant',
    timestamp: '2026-01-01T00:00:00Z',
    plainContent: 'placeholder',
    renderedContent: JSON.stringify({ kind: 'gate', request }),
    provider: null,
    model: null,
    kind: 'approval',
    executionId: 'exec-1',
    sequence: 1,
    import: null,
    targets: [],
  }
}

describe('gateRequestOf', () => {
  it('parses the RunStep::Gate carried in renderedContent', () => {
    const request: GateRequest = { kind: 'runInstall', command: 'npm i' }
    const parsed = gateRequestOf(approvalMessage(request))
    expect(parsed).toEqual(request)
  })

  it('returns null for a non-approval message', () => {
    const msg = approvalMessage({ kind: 'runInstall', command: 'npm i' })
    msg.kind = 'assistant'
    expect(gateRequestOf(msg)).toBeNull()
  })

  it('returns null when renderedContent is missing', () => {
    const msg = approvalMessage({ kind: 'runInstall', command: 'npm i' })
    msg.renderedContent = null
    expect(gateRequestOf(msg)).toBeNull()
  })

  it('returns null for unparseable JSON rather than throwing', () => {
    const msg = approvalMessage({ kind: 'runInstall', command: 'npm i' })
    msg.renderedContent = '{ not json'
    expect(gateRequestOf(msg)).toBeNull()
  })

  it('returns null when renderedContent is valid JSON but not a gate step', () => {
    const msg = approvalMessage({ kind: 'runInstall', command: 'npm i' })
    msg.renderedContent = JSON.stringify({ kind: 'note', text: 'hi' })
    expect(gateRequestOf(msg)).toBeNull()
  })
})

describe('gateSummary', () => {
  it('is specific to each request kind, not a generic sentence', () => {
    const cases: [GateRequest, RegExp][] = [
      [{ kind: 'addDependency', name: 'lodash' }, /lodash/],
      [{ kind: 'runInstall', command: 'npm i' }, /npm i/],
      [{ kind: 'networkAccess', target: 'api.example.com' }, /api\.example\.com/],
      [{ kind: 'deleteFiles', paths: ['a.txt'] }, /a\.txt/],
      [{ kind: 'deleteFiles', paths: ['a.txt', 'b.txt'] }, /2 files/],
      [{ kind: 'outsideRepo', path: 'C:/other' }, /C:\/other/],
      [{ kind: 'publish', effect: 'send commits to the server' }, /send commits to the server/],
      [{ kind: 'unclassified', summary: 'do a weird thing' }, /do a weird thing/],
    ]
    for (const [request, expected] of cases) {
      expect(gateSummary(request)).toMatch(expected)
    }
  })

  it('says a publish leaves the project, since that is what undo cannot take back', () => {
    // The product's promise is that the agent never silently pushes, posts or
    // merges. A publishing command arrives at the gate looking like any other
    // shell write, so this wording is the only thing telling the person what
    // they are actually approving.
    const summary = gateSummary({ kind: 'publish', effect: 'merge a pull request' })
    expect(summary).toMatch(/out of this project/i)
    expect(summary).toMatch(/merge a pull request/)
  })
})

describe('gateOptions', () => {
  it('always offers exactly three answers: allow, find another way, stop', () => {
    const options = gateOptions()
    expect(options.map((o) => o.answer)).toEqual(['allowOnce', 'findAnotherWay', 'stopRun'])
    for (const o of options) {
      expect(o.label.length).toBeGreaterThan(0)
    }
  })
})

describe('gateAnswerNoteFor', () => {
  const msg = (id: string, kind: string, content: string, executionId: string | null = 'e1') =>
    ({ messageId: id, executionId, plainContent: content, kind }) as never

  const gate = { messageId: 'g', executionId: 'e1' } as never

  it('finds the note recording the answer', () => {
    const messages = [msg('g', 'approval', 'may I?'), msg('n', 'system', 'You allowed this, just this once.')]
    expect(gateAnswerNoteFor(messages, gate)).toBe('You allowed this, just this once.')
  })

  it('ignores a note from a different agent', () => {
    const messages = [msg('g', 'approval', 'may I?'), msg('n', 'system', 'You stopped the run here.', 'other')]
    expect(gateAnswerNoteFor(messages, gate)).toBeNull()
  })

  it('ignores a note that came before the gate', () => {
    const messages = [msg('n', 'system', 'You stopped the run here.'), msg('g', 'approval', 'may I?')]
    expect(gateAnswerNoteFor(messages, gate)).toBeNull()
  })

  it('cannot be settled by an agent quoting the phrase', () => {
    // Exact match on a system note, not a substring of any message: otherwise
    // an assistant reply mentioning the wording would retire a live gate.
    const messages = [
      msg('g', 'approval', 'may I?'),
      msg('a', 'assistant', 'I will note that "You allowed this, just this once." and continue.'),
    ]
    expect(gateAnswerNoteFor(messages, gate)).toBeNull()
  })

  it('is null while the gate is genuinely unanswered', () => {
    expect(gateAnswerNoteFor([msg('g', 'approval', 'may I?')], gate)).toBeNull()
  })
})
