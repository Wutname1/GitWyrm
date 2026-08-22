import { describe, expect, it } from 'vitest'
import type { SessionMessage } from '@/lib/bindings'
import { displayText, foldThoughtSummaries } from './agentDeskTranscript'

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

describe('displayText', () => {
  /**
   * Regression test for the bug where a bridged run-event message (one the
   * backend's `agentdesk::bridge::build_message` produced, carrying the
   * step's entire `RunStep` as JSON in `renderedContent` alongside the
   * plain-language sentence in `plainContent`) rendered its raw JSON
   * envelope as markdown -- a "what folder are you in?" answer showed up as
   * ~20 messages each printing a `{"kind":"note","text":"..."}` fragment.
   * `executionId` is set if and only if the message came from the bridge
   * (see `SessionMessage.execution_id`'s doc comment: "`None` for messages
   * not produced by an execution"), so it is the discriminator: any message
   * with an `executionId` must display `plainContent`, never
   * `renderedContent`, no matter what JSON that field holds.
   */
  it('never displays renderedContent as markdown for a bridged run-event message', () => {
    const bridged = msg('m1', 'assistant')
    bridged.plainContent = 'C:\\code\\GitWyrm\\.claude\\worktrees\\agent-desk-docs-2f7a11'
    bridged.renderedContent = '{"kind":"note","text":"C:\\\\code\\\\GitWyrm..."}'
    bridged.executionId = 'exec-1'

    const result = displayText(bridged)
    expect(result.isMarkdown).toBe(false)
    expect(result.text).toBe(bridged.plainContent)
  })

  it('renders renderedContent as markdown for a message with no executionId', () => {
    const imported = msg('m2', 'assistant')
    imported.executionId = null
    imported.renderedContent = '**bold answer**'

    const result = displayText(imported)
    expect(result.isMarkdown).toBe(true)
    expect(result.text).toBe('**bold answer**')
  })

  it('falls back to plainContent when renderedContent is absent, regardless of executionId', () => {
    const noRendered = msg('m3', 'user')
    noRendered.executionId = null
    noRendered.renderedContent = null
    noRendered.plainContent = 'hello'

    expect(displayText(noRendered)).toEqual({ text: 'hello', isMarkdown: false })
  })
})
