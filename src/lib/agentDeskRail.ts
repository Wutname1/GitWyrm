import type { SessionMessage } from '@/lib/bindings'

/**
 * Message rail math (tasks.md 5.1, 5.3): where each tick sits along the
 * rail, and how a message's text is cut down to a fixed number of lines for
 * the hover/focus popup snippet.
 *
 * Pulled out of `ConversationPane` so both are covered by fast `.test.ts`
 * unit tests -- this project's `vitest.config.ts` runs `src/**\/*.test.ts`
 * in a Node environment with no DOM, so the actual layout pass (measuring
 * message offsets after render/resize) cannot run here; only the pure math
 * that turns offsets into tick positions can (see
 * `src/lib/agentSessionGrouping.ts` for the same split). The component is
 * responsible for measuring real offsets (via `getBoundingClientRect` after
 * layout and on resize) and calling `computeRailTicks` with the result.
 */

export interface RailTickInput {
  messageId: string
  /** The message's top offset within the scrollable transcript, in pixels. */
  offsetTop: number
}

export interface RailTick {
  messageId: string
  /** 0-1 position along the rail's track. */
  position: number
  isCurrent: boolean
}

/**
 * Turns raw offsets (already measured against the transcript's own scroll
 * height) into normalized 0-1 tick positions, marking the last one current
 * by default (the newest user message, matching the mockup's
 * `.ag-history-jump.is-current` on the most recent entry).
 *
 * Degenerate inputs collapse to safe values instead of dividing by zero:
 * zero ticks produce an empty rail; a single tick (or a transcript with no
 * scrollable range) sits at the top.
 */
export function computeRailTicks(inputs: RailTickInput[], scrollHeight: number, currentMessageId?: string): RailTick[] {
  if (inputs.length === 0) return []
  const span = scrollHeight > 0 ? scrollHeight : 1
  const fallbackCurrent = inputs[inputs.length - 1]?.messageId
  const current = currentMessageId ?? fallbackCurrent
  return inputs.map((input) => ({
    messageId: input.messageId,
    position: span > 0 ? Math.min(1, Math.max(0, input.offsetTop / span)) : 0,
    isCurrent: input.messageId === current,
  }))
}

/**
 * Cuts a message's plain-text content down to at most `maxLines` lines for
 * the rail popup, truncating by line count rather than by shrinking the
 * type size (tasks.md 5.3: "Truncate snippets by lines, not by shrinking
 * type"). A line is anything separated by `\n`; long single lines are left
 * to the popup's own text wrapping/clamping (`line-clamp`) rather than being
 * cut here, since character-width truncation would need to know the font.
 */
export function truncateSnippet(content: string, maxLines: number): { text: string; truncated: boolean } {
  const lines = content.split('\n')
  if (lines.length <= maxLines) {
    return { text: content, truncated: false }
  }
  return { text: lines.slice(0, maxLines).join('\n'), truncated: true }
}

/** Every user message from a transcript, in transcript order -- what the rail tracks. */
export function userMessagesForRail(messages: SessionMessage[]): SessionMessage[] {
  return messages.filter((m) => m.role === 'user')
}

/**
 * Which message the reader is currently looking at, given a scroll position.
 *
 * `computeRailTicks` has always accepted a `currentMessageId`, and the caller
 * never passed one -- so the "you are here" tick fell back to the newest
 * message and sat there however far you scrolled. The one affordance for
 * orienting yourself in a long transcript could not orient.
 *
 * The current message is the last one whose top has passed the top of the
 * viewport: that is the message whose content fills the screen, rather than
 * the next one about to appear. Before the first message's top is reached, the
 * first message is current.
 */
export function currentMessageForScroll(inputs: RailTickInput[], scrollTop: number): string | undefined {
  if (inputs.length === 0) return undefined
  let current = inputs[0].messageId
  for (const input of inputs) {
    if (input.offsetTop <= scrollTop) current = input.messageId
    else break
  }
  return current
}
