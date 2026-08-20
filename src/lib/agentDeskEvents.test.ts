import { describe, expect, it } from 'vitest'
import type { SessionMessage } from '@/lib/bindings'
import { groupEventStacks, splitEventHeadline } from './agentDeskEvents'

function toolMessage(id: string, plainContent: string, targets: SessionMessage['targets'] = []): SessionMessage {
  return {
    messageId: id,
    segmentId: 'seg-1',
    role: 'assistant',
    timestamp: '2026-08-19T00:00:00Z',
    plainContent,
    renderedContent: null,
    provider: null,
    model: null,
    kind: 'tool',
    executionId: 'exec-1',
    sequence: 1,
    import: null,
    targets,
  }
}

function assistantMessage(id: string, plainContent = 'reply'): SessionMessage {
  return {
    messageId: id,
    segmentId: 'seg-1',
    role: 'assistant',
    timestamp: '2026-08-19T00:00:00Z',
    plainContent,
    renderedContent: null,
    provider: null,
    model: null,
    kind: 'assistant',
    executionId: 'exec-1',
    sequence: 1,
    import: null,
    targets: [],
  }
}

describe('splitEventHeadline', () => {
  it('splits on the first " - " separator', () => {
    expect(splitEventHeadline('Research finished - the panic starts in settings.rs')).toEqual({
      headline: 'Research finished',
      detail: 'the panic starts in settings.rs',
    })
  })

  it('splits on ": " when it comes before " - "', () => {
    expect(splitEventHeadline('UI worker changed 2 files: adding an inline recovery state')).toEqual({
      headline: 'UI worker changed 2 files',
      detail: 'adding an inline recovery state',
    })
  })

  it('returns the whole trimmed sentence as headline with no detail when there is no separator', () => {
    expect(splitEventHeadline('  Your files are untouched  ')).toEqual({
      headline: 'Your files are untouched',
      detail: '',
    })
  })
})

describe('groupEventStacks', () => {
  it('groups a run of consecutive tool messages into one stack anchored after the prior message', () => {
    const messages = [
      assistantMessage('m1', 'Plan announced'),
      toolMessage('m2', 'Research finished - the panic starts in settings.rs'),
      toolMessage('m3', 'UI worker changed 2 files - adding an inline recovery state'),
      assistantMessage('m4', 'Wrapping up'),
    ]

    const groups = groupEventStacks(messages)
    expect(groups).toHaveLength(1)
    expect(groups[0].afterMessageId).toBe('m1')
    expect(groups[0].items).toHaveLength(2)
    expect(groups[0].items[0].headline).toBe('Research finished')
    expect(groups[0].items[1].headline).toBe('UI worker changed 2 files')
  })

  it('anchors the first group to the run\'s own first message id when the transcript starts with tool messages', () => {
    const messages = [toolMessage('m1', 'Started up'), assistantMessage('m2')]
    const groups = groupEventStacks(messages)
    expect(groups[0].afterMessageId).toBe('m1')
  })

  it('splits two separate runs into two groups when a non-tool message interrupts them', () => {
    const messages = [
      toolMessage('m1', 'First run'),
      assistantMessage('m2'),
      toolMessage('m3', 'Second run'),
    ]
    const groups = groupEventStacks(messages)
    expect(groups).toHaveLength(2)
    expect(groups[0].afterMessageId).toBe('m1')
    expect(groups[1].afterMessageId).toBe('m2')
  })

  it('carries the first target through as the item target, or null when there are none', () => {
    const target: SessionMessage['targets'][number] = { kind: 'file', path: 'src/lib/foo.ts' }
    const messages = [toolMessage('m1', 'Changed a file', [target]), toolMessage('m2', 'No target here')]
    const groups = groupEventStacks(messages)
    expect(groups[0].items[0].target).toEqual(target)
    expect(groups[0].items[1].target).toBeNull()
  })

  it('returns no groups for a transcript with no tool messages', () => {
    expect(groupEventStacks([assistantMessage('m1'), assistantMessage('m2')])).toEqual([])
  })
})
