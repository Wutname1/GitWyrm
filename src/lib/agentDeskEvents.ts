import type { MessageTarget, SessionMessage } from '@/lib/bindings'

/**
 * Groups a transcript's `tool`-kind messages into compact "event stack"
 * runs, matching the mockup's `.ag-event-stack` (a short list of activity
 * lines indented under a message, each with an icon, a bold headline plus
 * detail text, and an optional link).
 *
 * There is no dedicated `MessageKind` for "event" -- `src-tauri/src/agentdesk/
 * bridge.rs::message_kind_for_step` maps `RunStep::Edit`/`RunStep::Check` (the
 * two step kinds that produce day-to-day activity, as opposed to a plan, a
 * gate, or a chat line) to `MessageKind::Tool`, and fills `plain_content`
 * with a plain-language sentence via `summarize()`. So the event stack is a
 * *display* grouping of existing `tool` messages, not a new wire shape: a run
 * of consecutive `tool` messages (uninterrupted by any other kind) becomes
 * one stack, rendered after the run's last message in transcript order.
 *
 * Pulled out of the component so the grouping is covered by a fast
 * `.test.ts` unit test (this project's `vitest.config.ts` runs
 * `src/**\/*.test.ts` in a Node environment with no DOM -- see
 * `src/lib/agentDeskRail.ts` for the same split).
 */
export interface EventStackItem {
  messageId: string
  /** Execution that produced the event, so a file link opens in that helper's own worktree. */
  executionId: string | null
  headline: string
  detail: string
  target: MessageTarget | null
}

export interface EventStackGroup {
  /** The transcript-order key: this group renders immediately after the message with this id. */
  afterMessageId: string
  items: EventStackItem[]
}

/**
 * Splits a headline sentence from its detail. `summarize()` on the Rust side
 * produces one plain sentence (e.g. "UI worker changed 2 files, adding an
 * inline recovery state") -- the mockup's convention bolds a short lead
 * clause and mutes the rest, separated by the first " - " or ": ". When
 * neither separator is present, the whole sentence is the headline and the
 * detail is empty.
 */
export function splitEventHeadline(text: string): { headline: string; detail: string } {
  const trimmed = text.trim()
  const separators = [' - ', ': ']
  let bestIndex = -1
  let bestSeparator = ''
  for (const sep of separators) {
    const idx = trimmed.indexOf(sep)
    if (idx > 0 && (bestIndex === -1 || idx < bestIndex)) {
      bestIndex = idx
      bestSeparator = sep
    }
  }
  if (bestIndex === -1) {
    return { headline: trimmed, detail: '' }
  }
  return {
    headline: trimmed.slice(0, bestIndex),
    detail: trimmed.slice(bestIndex + bestSeparator.length).trim(),
  }
}

/**
 * Groups every run of consecutive `tool` messages in `messages` into
 * `EventStackGroup`s, keyed by the message id the group should render after
 * (the last message before the run started, or the first tool message's own
 * id if the transcript starts with one).
 */
export function groupEventStacks(messages: SessionMessage[]): EventStackGroup[] {
  const groups: EventStackGroup[] = []
  let current: EventStackItem[] = []
  let anchorId: string | null = null
  let previousId: string | null = null

  const flush = () => {
    if (current.length > 0 && anchorId) {
      groups.push({ afterMessageId: anchorId, items: current })
    }
    current = []
    anchorId = null
  }

  for (const message of messages) {
    if (message.kind === 'tool') {
      if (current.length === 0) {
        anchorId = previousId ?? message.messageId
      }
      const { headline, detail } = splitEventHeadline(message.plainContent)
      current.push({
        messageId: message.messageId,
        executionId: message.executionId,
        headline,
        detail,
        target: message.targets[0] ?? null,
      })
    } else {
      flush()
    }
    previousId = message.messageId
  }
  flush()

  return groups
}
