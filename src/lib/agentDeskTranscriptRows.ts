import type { SessionMessage } from '@/lib/bindings'
import { groupEventStacks } from '@/lib/agentDeskEvents'
import { foldThoughtSummaries } from '@/lib/agentDeskTranscript'

/**
 * The exact row-selection rule `ConversationPane`'s `messages.flatMap` in the
 * transcript body applies, pulled out as a pure function so it can be
 * regression-tested without mounting the component (that flatMap builds JSX
 * directly and has no other seam to test through).
 *
 * Mirror this function's body whenever the flatMap in `ConversationPane.tsx`
 * changes -- it exists so "does every non-tool, non-folded-thought message
 * still produce a row" can be asserted directly against `foldThoughtSummaries`
 * and `groupEventStacks`'s real output, the same two functions the component
 * calls, instead of re-deriving the answer by hand per test.
 */
export function transcriptRowMessageIds(messages: SessionMessage[]): string[] {
  const { folded: foldedThoughtIds } = foldThoughtSummaries(messages)
  const ids: string[] = []
  for (const m of messages) {
    if (m.kind === 'tool') continue
    if (foldedThoughtIds.has(m.messageId)) continue
    ids.push(m.messageId)
  }
  return ids
}

/** Every event-stack group's anchor message id, for asserting where activity feeds attach. */
export function transcriptEventAnchors(messages: SessionMessage[]): string[] {
  return groupEventStacks(messages).map((g) => g.afterMessageId)
}
