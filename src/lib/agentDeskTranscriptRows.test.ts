import { describe, expect, it } from 'vitest'
import type { SessionMessage } from '@/lib/bindings'
import { transcriptEventAnchors, transcriptRowMessageIds } from './agentDeskTranscriptRows'

function msg(id: string, kind: SessionMessage['kind'], overrides: Partial<SessionMessage> = {}): SessionMessage {
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
    ...overrides,
  }
}

describe('transcriptRowMessageIds', () => {
  // The reported bug: send a user message, then a second one, before any
  // reply has arrived (execution still `preparing`) -- both must survive the
  // fold/skip pipeline into rendered rows. Neither `foldThoughtSummaries` nor
  // `groupEventStacks` has anything to fold or skip here (no `thoughtSummary`
  // or `tool` messages exist yet), so this is the simplest possible case, and
  // the one that matches what the user actually saw go missing.
  it('keeps both plain user messages sent back-to-back with no reply yet', () => {
    const messages = [msg('u1', 'user'), msg('u2', 'user')]
    expect(transcriptRowMessageIds(messages)).toEqual(['u1', 'u2'])
  })

  it('keeps a single user message with nothing else in the transcript', () => {
    const messages = [msg('u1', 'user')]
    expect(transcriptRowMessageIds(messages)).toEqual(['u1'])
  })

  it('drops tool messages from the row list but keeps every other kind', () => {
    const messages = [
      msg('u1', 'user'),
      msg('tool1', 'tool', { role: 'assistant' }),
      msg('tool2', 'tool', { role: 'assistant' }),
      msg('a1', 'assistant'),
    ]
    expect(transcriptRowMessageIds(messages)).toEqual(['u1', 'a1'])
  })

  it('drops a thoughtSummary folded into the reply that follows it, but keeps the user message before it', () => {
    const messages = [msg('u1', 'user'), msg('t1', 'thoughtSummary', { role: 'assistant' }), msg('a1', 'assistant')]
    expect(transcriptRowMessageIds(messages)).toEqual(['u1', 'a1'])
  })

  it('a realistic mixed transcript keeps every user and assistant message in order', () => {
    const messages = [
      msg('u1', 'user'),
      msg('t1', 'thoughtSummary', { role: 'assistant' }),
      msg('a1', 'assistant'),
      msg('tool1', 'tool', { role: 'assistant' }),
      msg('u2', 'user'),
      msg('u3', 'user'),
    ]
    expect(transcriptRowMessageIds(messages)).toEqual(['u1', 'a1', 'u2', 'u3'])
  })

  it('an empty transcript produces no rows', () => {
    expect(transcriptRowMessageIds([])).toEqual([])
  })
})

describe('transcriptEventAnchors', () => {
  it('produces no event-stack anchors when the transcript has no tool messages', () => {
    const messages = [msg('u1', 'user'), msg('u2', 'user')]
    expect(transcriptEventAnchors(messages)).toEqual([])
  })
})
