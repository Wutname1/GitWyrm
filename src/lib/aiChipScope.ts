/**
 * What the titlebar AI chip is allowed to claim it governs.
 *
 * Two different AI settings live in the Agent Desk window and they are not
 * the same thing:
 *
 * - The **chip** is one global choice behind everything GitWyrm writes for
 *   you: commit messages, spec drafts, conflict help, and Agent Desk's own
 *   "send this result back to the spec".
 * - A **chat** picks its own backend beside its message box. Nothing on that
 *   path reads the chip's on/off switch.
 *
 * So the chip could read "AI · off" while a chat below it was mid-run on a
 * different provider. The words are the whole fix -- the wiring was already
 * correct, it was the label that claimed to speak for both.
 *
 * Spec Desk has only the one AI, so there the chip really does speak for
 * everything and keeps its original wording.
 */

/**
 * `all` -- this window has one AI and the chip speaks for it (Spec Desk).
 * `writing` -- this window also has per-chat AI, so the chip names only what
 * GitWyrm writes for you and says so.
 */
export type AiChipScope = 'all' | 'writing'

/** The noun the chip uses for itself. */
export function aiChipNoun(scope: AiChipScope): string {
  return scope === 'writing' ? 'Writing help' : 'AI'
}

/**
 * The sentence appended to every tooltip in a window that also has per-chat
 * AI, so the boundary is stated wherever someone stops to ask what the chip
 * governs. Empty where the chip genuinely is the only AI.
 */
export function aiChipScopeNote(scope: AiChipScope): string {
  return scope === 'writing' ? ' Each chat picks its own AI beside its message box.' : ''
}

/**
 * What turning the chip off actually stops.
 *
 * The old copy said "Copying handoffs still works", which in Agent Desk read
 * as the full list of what survived -- implying chats did not. They do.
 */
export function aiChipOffDescription(scope: AiChipScope, providerShort: string): string {
  return scope === 'writing'
    ? `${providerShort} stays signed in. Your chats are not affected.`
    : `${providerShort} stays signed in. Copying handoffs still works.`
}
