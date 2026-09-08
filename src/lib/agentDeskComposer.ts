import type { ExecutionMode, ExecutionTeam } from '@/lib/bindings'

/** UI-facing operating mode, matching the mockup's `data-mode` values. */
export type ComposerMode = 'Ask' | 'Plan' | 'Auto'

/** UI-facing team shape, matching the mockup's Solo/Lead + helpers choice. */
export type ComposerTeam = 'solo' | 'helpers'


/**
 * Plain-language note shown next to the mode pills (tasks.md 6.1).
 *
 * Copy is verbatim from the real mockup's `.ag-mode-note` text per source
 * (`data-mode-note`) for each `data-mode` button, not reworded -- see
 * `docs/agent-desk/agent-desk-mockup.html`.
 */
export const MODE_NOTES: Record<ComposerMode, string> = {
  Ask: 'Read and answer only; no helpers, file changes or commands',
  Plan: 'Lead may inspect and draft a team plan; you press Start',
  Auto: 'Lead may use up to 3 helpers; asks before file changes or commands',
}

/** Maps the UI's three-way mode pill to the backend's `ExecutionMode`. */
export function modeToExecutionMode(mode: ComposerMode): ExecutionMode {
  switch (mode) {
    case 'Ask':
      return 'ask'
    case 'Plan':
      return 'plan'
    case 'Auto':
      return 'auto'
  }
}

/** Maps the UI's Solo/Lead + helpers choice to the backend's `ExecutionTeam`. */
export function teamToExecutionTeam(team: ComposerTeam): ExecutionTeam {
  return team === 'solo' ? 'solo' : 'lead'
}

/**
 * Whether the composer's Send action should be allowed right now.
 *
 * Pulled out of `SessionComposer` so the duplicate-send guard (tasks.md 6.4:
 * "prevent duplicate sends while accepting the message") is covered by a
 * fast unit test rather than only by clicking the button in the app. The
 * draft's raw text is still accepted into state as the user types even
 * while a previous send is in flight -- this only gates the button/submit
 * action, never the textarea itself.
 */
export function canSendComposerDraft(input: {
  draft: string
  sessionId: string | null
  sending: boolean
}): boolean {
  return input.draft.trim().length > 0 && !input.sending && input.sessionId != null
}

/**
 * Why Plan and Auto are unavailable in a chat that cannot change files.
 *
 * Said once, here, because the mode pills' tooltip and the note beside them
 * must not drift apart, and because a control that refuses a click owes the
 * person a reason rather than just going quiet.
 */
/**
 * Whether a mode is offered but refused for this chat.
 *
 * A chat started from "Review this pull request" or "Explain this issue" is
 * read-only in the engine: the write and shell tools are denied when the CLI
 * launches, and a mode can never widen what the intent allows. Picking Plan
 * or Auto there changes nothing.
 *
 * Shared by the composer's pills and the new-chat cards. Each used to decide
 * this for itself, and the cards did not decide it at all -- so on a Review
 * chat the two controls sat an inch apart disagreeing, one refusing a mode
 * the other lit up on click.
 */
export function isModeBlocked(mode: ComposerMode, canWrite: boolean): boolean {
  return !canWrite && mode !== 'Ask'
}

export const READ_ONLY_REASON = 'This chat only reads and explains, so it cannot change files.'

/**
 * Whether picking a team of helpers would do anything, given the mode.
 *
 * It would not, in Ask. Helpers are proposed by an instruction the backend
 * only adds for Plan and Auto -- in Ask it adds nothing, so a lead has no way
 * to hand work out and the run is solo whatever this says. `MODE_NOTES.Ask`
 * says as much on the card directly above ("no helpers"), which is the screen
 * contradicting itself in text a person can read without scrolling.
 *
 * The condition is the mode, not whether the chat may write. A read-only chat
 * is covered because `isModeBlocked` already pins it to Ask -- but a perfectly
 * writable chat whose owner picked Ask has exactly the same problem, and
 * gating on `canWrite` would leave that one lying.
 *
 * It is not a rare state. The team defaults to `helpers`, so every read-only
 * chat opened showing a team selected, and the composer beside it read "A lead
 * agent, up to 3 helpers", for a run that could never have one.
 */
export function isTeamBlocked(mode: ComposerMode): boolean {
  return mode === 'Ask'
}

export const TEAM_NEEDS_MODE_REASON =
  'Ask mode works alone. Pick Plan or Auto to use a team.'
