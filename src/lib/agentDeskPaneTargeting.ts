/**
 * Pure decision logic for "which pane does this session land in" (tasks.md
 * 4.3, 4.6, 5.4, 5.5). Pulled out of the view/store so the targeting rules --
 * especially "an already-open session focuses its pane instead of opening a
 * second copy of itself" -- are covered by a fast unit test instead of only
 * by clicking around a live two-pane layout.
 */

export type PaneId = 'primary' | 'secondary'

export interface PaneTargetingInput {
  split: boolean
  activePane: PaneId
  primarySessionId: string | null
  secondarySessionId: string | null
  /** The session about to be selected (sidebar click, New chat, import, etc.). */
  sessionId: string
}

export type PaneTargetingResult =
  /** Write `sessionId` into `pane`. */
  | { action: 'open'; pane: PaneId }
  /** The session is already showing in `pane` -- just move focus there, do not open a duplicate. */
  | { action: 'focus'; pane: PaneId }

/**
 * Resolves where a session selection should land.
 *
 * Rules (architecture.md section 7, tasks.md 5.4/5.5):
 *   - Not split: always the single (primary) pane.
 *   - Split: if the session is already visible in either pane, focus that
 *     pane instead of opening a second copy of it -- even if the *other*
 *     pane is the active one right now.
 *   - Split, not already visible: open into the active pane, replacing
 *     whatever it currently shows.
 */
export function resolvePaneTarget(input: PaneTargetingInput): PaneTargetingResult {
  if (!input.split) {
    return { action: 'open', pane: 'primary' }
  }
  if (input.primarySessionId === input.sessionId) {
    return { action: 'focus', pane: 'primary' }
  }
  if (input.secondarySessionId === input.sessionId) {
    return { action: 'focus', pane: 'secondary' }
  }
  return { action: 'open', pane: input.activePane }
}

export interface SplitCollapseInput {
  activePane: PaneId
  primarySessionId: string | null
  secondarySessionId: string | null
}

export interface SplitCollapseResult {
  activePane: 'primary'
  primarySessionId: string | null
  secondarySessionId: null
}

/**
 * Resolves the layout after collapsing Split View (tasks.md 5.6): keeps the
 * *active* pane's session, promoting the secondary pane's session into the
 * primary slot when the secondary pane was the active one.
 */
export function resolveSplitCollapse(input: SplitCollapseInput): SplitCollapseResult {
  const keepSessionId = input.activePane === 'secondary' ? input.secondarySessionId : input.primarySessionId
  return { activePane: 'primary', primarySessionId: keepSessionId, secondarySessionId: null }
}

/** The other pane, for `Ctrl+1`/`Ctrl+2` and any "focus the other pane" command. */
export function otherPane(pane: PaneId): PaneId {
  return pane === 'primary' ? 'secondary' : 'primary'
}
