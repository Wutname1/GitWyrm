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
  Ask: 'Read and answer only; no helpers or file changes',
  Plan: 'Lead may inspect and draft a graph; you start it',
  Auto: 'Lead may use up to 3 helpers and asks before risky actions',
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
