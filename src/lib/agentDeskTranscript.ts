import type { SessionMessage } from '@/lib/bindings'

/**
 * Decides which messages a `thoughtSummary` message should fold into,
 * matching the mockup's `.ag-thought`: it always renders *inside* the same
 * message block as the reply that follows it, never as its own separate
 * row in the transcript.
 *
 * A `thoughtSummary` message folds into the next message in transcript
 * order when that next message is `assistant` or `result` (the two kinds a
 * lead/reviewer's own reply can carry) -- otherwise (last message in the
 * transcript, or followed by something else entirely) it renders standalone
 * with the same `ThoughtBlock` treatment, so a mid-stream thought is never
 * silently dropped while its reply is still arriving.
 *
 * Returns a map from a message id that should render a thought block above
 * it to the `thoughtSummary` message supplying that block's text, plus the
 * set of `thoughtSummary` message ids that were folded (and so must be
 * skipped when the transcript is walked for normal rows).
 */
export interface FoldedThoughts {
  /** messageId (the reply, or the thought itself when standalone) -> the thoughtSummary message to render above it. */
  thoughtFor: Map<string, SessionMessage>
  /** thoughtSummary message ids that were folded into a later reply and must not render as their own row. */
  folded: Set<string>
}

export function foldThoughtSummaries(messages: SessionMessage[]): FoldedThoughts {
  const thoughtFor = new Map<string, SessionMessage>()
  const folded = new Set<string>()

  for (let i = 0; i < messages.length; i++) {
    const message = messages[i]
    if (message.kind !== 'thoughtSummary') continue
    const next = messages[i + 1]
    if (next && (next.kind === 'assistant' || next.kind === 'result')) {
      thoughtFor.set(next.messageId, message)
      folded.add(message.messageId)
    } else {
      thoughtFor.set(message.messageId, message)
    }
  }

  return { thoughtFor, folded }
}
