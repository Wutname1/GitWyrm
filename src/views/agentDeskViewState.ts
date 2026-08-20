/**
 * Pure state-selection logic for `AgentDeskView`'s explicit shell states
 * (task 2.2: opening, empty, load-failed, no-repository), pulled out of the
 * component so it can be unit tested.
 *
 * The project's test setup (`vitest.config.ts`) is scoped to `src/**\/*.test.ts`
 * in a Node environment on purpose -- there is no DOM/React Testing Library
 * dependency wired up yet, so component-level render tests are out of scope
 * for this change. This module exists so the branching that decides *which*
 * state renders is still covered by a fast, deterministic unit test in the
 * pattern the codebase already uses (see `src/lib/agentSessionGrouping.ts`).
 */

export type AgentDeskShellState =
  | 'no-repository'
  | 'load-failed'
  | 'opening'
  | 'empty'
  | 'ready'

export interface AgentDeskShellInputs {
  /** True when the window was opened with no repository path at all. */
  hasRepoPath: boolean
  /** Set once opening the repository failed. */
  repoError: string | null
  /** The opened repo, once `open_repo` has resolved. */
  repoReady: boolean
  /** True while the session header list is still loading. */
  sessionsLoading: boolean
  /** Number of session headers loaded so far. */
  sessionCount: number
  /** A session is selected (from the list, or auto-selected on load). */
  hasSelection: boolean
}

/**
 * Resolves which of the four explicit states (plus the normal "ready" case)
 * `AgentDeskView` should render.
 *
 * Order matters: a missing repo path and a failed open both take priority
 * over "opening", which takes priority over "empty" (no chats yet), which
 * only applies once nothing is loading and nothing is selected -- otherwise
 * a session that legitimately has zero prior chats would flash the empty
 * state before its own first render settles.
 */
export function resolveAgentDeskShellState(inputs: AgentDeskShellInputs): AgentDeskShellState {
  if (!inputs.hasRepoPath) return 'no-repository'
  if (inputs.repoError) return 'load-failed'
  if (!inputs.repoReady) return 'opening'
  if (!inputs.sessionsLoading && inputs.sessionCount === 0 && !inputs.hasSelection) return 'empty'
  return 'ready'
}
