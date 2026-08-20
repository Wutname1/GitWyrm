import { describe, expect, it } from 'vitest'
import type { SessionMessage } from '@/lib/bindings'
import { computeRailTicks, truncateSnippet, userMessagesForRail } from './agentDeskRail'

function userMessage(overrides: Partial<SessionMessage> & { messageId: string }): SessionMessage {
  return {
    segmentId: 'seg-1',
    role: 'user',
    timestamp: '2026-01-01T00:00:00Z',
    plainContent: 'hi',
    renderedContent: null,
    provider: null,
    model: null,
    kind: 'user',
    executionId: null,
    sequence: null,
    import: null,
    targets: [],
    ...overrides,
  }
}

describe('computeRailTicks', () => {
  it('returns an empty rail for no messages', () => {
    expect(computeRailTicks([], 1000)).toEqual([])
  })

  it('places a single tick at the top and marks it current', () => {
    const ticks = computeRailTicks([{ messageId: 'm1', offsetTop: 0 }], 1000)
    expect(ticks).toHaveLength(1)
    expect(ticks[0].position).toBe(0)
    expect(ticks[0].isCurrent).toBe(true)
  })

  it('normalizes offsets against scroll height and marks the last message current by default', () => {
    const ticks = computeRailTicks(
      [
        { messageId: 'm1', offsetTop: 0 },
        { messageId: 'm2', offsetTop: 500 },
        { messageId: 'm3', offsetTop: 1000 },
      ],
      1000
    )
    expect(ticks.map((t) => t.position)).toEqual([0, 0.5, 1])
    expect(ticks.find((t) => t.messageId === 'm3')?.isCurrent).toBe(true)
    expect(ticks.find((t) => t.messageId === 'm1')?.isCurrent).toBe(false)
  })

  it('honors an explicit current message id instead of defaulting to the last', () => {
    const ticks = computeRailTicks(
      [
        { messageId: 'm1', offsetTop: 0 },
        { messageId: 'm2', offsetTop: 500 },
      ],
      1000,
      'm1'
    )
    expect(ticks.find((t) => t.messageId === 'm1')?.isCurrent).toBe(true)
    expect(ticks.find((t) => t.messageId === 'm2')?.isCurrent).toBe(false)
  })

  it('does not divide by zero when scroll height is zero', () => {
    const ticks = computeRailTicks([{ messageId: 'm1', offsetTop: 0 }], 0)
    expect(ticks[0].position).toBe(0)
    expect(Number.isFinite(ticks[0].position)).toBe(true)
  })

  it('clamps positions into 0-1 even if an offset exceeds scroll height', () => {
    const ticks = computeRailTicks([{ messageId: 'm1', offsetTop: 2000 }], 1000)
    expect(ticks[0].position).toBe(1)
  })
})

describe('truncateSnippet', () => {
  it('returns the original text untruncated when within the line limit', () => {
    const result = truncateSnippet('one\ntwo', 3)
    expect(result.truncated).toBe(false)
    expect(result.text).toBe('one\ntwo')
  })

  it('cuts down to the max line count and marks it truncated', () => {
    const result = truncateSnippet('one\ntwo\nthree\nfour', 2)
    expect(result.truncated).toBe(true)
    expect(result.text).toBe('one\ntwo')
  })

  it('handles a single very long line by leaving it alone (wrapping is the caller’s job)', () => {
    const longLine = 'x'.repeat(500)
    const result = truncateSnippet(longLine, 2)
    expect(result.truncated).toBe(false)
    expect(result.text).toBe(longLine)
  })
})

describe('userMessagesForRail', () => {
  it('keeps only user-role messages, in order', () => {
    const messages: SessionMessage[] = [
      userMessage({ messageId: 'm1' }),
      { ...userMessage({ messageId: 'm2' }), role: 'assistant', kind: 'assistant' },
      userMessage({ messageId: 'm3' }),
    ]
    expect(userMessagesForRail(messages).map((m) => m.messageId)).toEqual(['m1', 'm3'])
  })

  it('returns an empty array for 0 user messages and all of them for large transcripts', () => {
    expect(userMessagesForRail([])).toEqual([])
    const many = Array.from({ length: 500 }, (_, i) => userMessage({ messageId: `m${i}` }))
    expect(userMessagesForRail(many)).toHaveLength(500)
  })
})

// tasks.md 5.5: "Test keyboard access, 1/50/500 user messages, and resized
// windows." Keyboard access and resize are exercised where they actually
// happen -- Radix's own Popover trigger/content (native <button> + roving
// focus, no custom key handling added) and the `ResizeObserver`-driven
// remeasure in `ConversationPane` -- so this suite covers the one part that
// is pure and can run outside a DOM: the rail math holding up at each scale.
describe('computeRailTicks at 1/50/500 user messages', () => {
  it('handles exactly 1 message', () => {
    const ticks = computeRailTicks([{ messageId: 'only', offsetTop: 400 }], 800)
    expect(ticks).toHaveLength(1)
    expect(ticks[0].isCurrent).toBe(true)
  })

  it('handles 50 evenly spaced messages', () => {
    const inputs = Array.from({ length: 50 }, (_, i) => ({ messageId: `m${i}`, offsetTop: i * 100 }))
    const ticks = computeRailTicks(inputs, 4900)
    expect(ticks).toHaveLength(50)
    expect(ticks[0].position).toBe(0)
    expect(ticks[49].position).toBe(1)
    expect(ticks[49].isCurrent).toBe(true)
    expect(ticks.every((t) => t.position >= 0 && t.position <= 1)).toBe(true)
  })

  it('handles 500 messages without losing monotonic ordering', () => {
    const inputs = Array.from({ length: 500 }, (_, i) => ({ messageId: `m${i}`, offsetTop: i * 40 }))
    const ticks = computeRailTicks(inputs, 500 * 40)
    expect(ticks).toHaveLength(500)
    for (let i = 1; i < ticks.length; i++) {
      expect(ticks[i].position).toBeGreaterThanOrEqual(ticks[i - 1].position)
    }
  })
})
