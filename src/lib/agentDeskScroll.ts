/**
 * Auto-follow logic for the transcript (tasks.md 4.5): new messages should
 * pull the view down only when the reader was already near the bottom --
 * scrolling up to reread something earlier must never get yanked back down
 * by the next streamed message.
 *
 * Pulled out of `ConversationPane` so the threshold math has a fast
 * `.test.ts` unit test; this project's `vitest.config.ts` runs
 * `src/**\/*.test.ts` in a Node environment with no DOM, so the scroll
 * container itself cannot be exercised here (see
 * `src/lib/agentSessionGrouping.ts` for the same split).
 */

/** How close to the bottom (in pixels of unseen content) still counts as "at the bottom". */
export const AUTO_FOLLOW_THRESHOLD_PX = 96

export interface ScrollMetrics {
  scrollTop: number
  scrollHeight: number
  clientHeight: number
}

/**
 * True when the reader is within `AUTO_FOLLOW_THRESHOLD_PX` of the bottom of
 * the transcript, i.e. it is safe to auto-scroll on the next new message.
 *
 * An empty/not-yet-overflowing transcript (`scrollHeight <= clientHeight`)
 * always counts as "at the bottom" -- there is nothing to scroll past, and a
 * brand new chat must not be treated as "the user scrolled away" just
 * because it has not overflowed yet.
 */
export function isNearBottom(metrics: ScrollMetrics): boolean {
  const unseen = metrics.scrollHeight - metrics.clientHeight - metrics.scrollTop
  return unseen <= AUTO_FOLLOW_THRESHOLD_PX
}
