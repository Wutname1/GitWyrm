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

/**
 * Decides what a message's body should render: the raw `plainContent`
 * sentence, or `renderedContent` as markdown.
 *
 * `renderedContent` is not one thing across every `SessionMessage` producer.
 * For a message the run bridge built (`agentdesk::bridge::build_message`,
 * `src-tauri/src/agentdesk/bridge.rs`), it is the message's entire `RunStep`
 * serialized to JSON -- a *data* field kept so a consumer can deserialize the
 * full typed step back (see that file's `map_run_step` doc comment and the
 * round-trip test in `bridge.rs`), never meant to be shown to a person. Every
 * other producer of `SessionMessage` (`commands/agent_desk.rs`,
 * `commands/agent_import.rs`) always sets `rendered_content: None`, so today
 * nothing else populates it with real markdown -- but the rule below does not
 * depend on that happening to be true; it depends on the one field the bridge
 * always sets alongside its JSON dump.
 *
 * The bridge is also the only producer that sets `execution_id` on a
 * message (see `SessionMessage.execution_id`'s own doc comment: "`None` for
 * messages not produced by an execution"), which makes `execution_id` a
 * reliable, already-existing discriminator: a message with an `executionId`
 * came from a bridged run event and must display `plainContent`, no matter
 * what `renderedContent` holds; a message with no `executionId` is free to
 * use `renderedContent` as markdown when present.
 */
export function displayText(message: SessionMessage): { text: string; isMarkdown: boolean } {
  if (message.renderedContent && message.executionId == null) {
    return { text: message.renderedContent, isMarkdown: true }
  }
  return { text: message.plainContent, isMarkdown: false }
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
