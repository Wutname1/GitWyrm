import type { SessionSource } from '@/lib/bindings'

/**
 * Maps a `SessionSource` (what a durable Agent Desk session started from) to
 * the action the *main* window should take to show it, once
 * `useAgentDeskSourceListener.ts` has focused that window via the
 * `agent-desk://open-source` event (`commands::agent_desk::agent_session_open_source`).
 *
 * Pulled out of the listener hook, same reason `agentDeskTargets.ts` pulls
 * its mapping out of the component: this project's `vitest.config.ts` runs
 * `src/**\/*.test.ts` in a Node environment with no DOM, so this pure
 * function is the only part of the "View source" bridge that is unit
 * testable, and it is also the single place that says which source kinds
 * are genuinely reachable today.
 *
 * Each kind routes to a destination that already exists in the main window
 * rather than a new one built for this bridge:
 *   - `issue`/`pullRequest` -> `uiStore.openGithubItem`, the same call the
 *     GitHub list/detail views use (`GithubContextPanel.tsx`).
 *   - `openSpecChange`/`openSpecTask` -> `selectChangeEverywhere`
 *     (`src/lib/specSync.ts`), the same broadcast the Spec Desk's own change
 *     list uses. There is no per-task selection surface anywhere in the app
 *     today (only per-change), so an `openSpecTask` source honestly lands on
 *     its parent change rather than pretending to jump to one task.
 *   - `commit` -> `uiStore.selectCommit` + `showGraph`, the same pair the
 *     graph's own commit click uses.
 *   - `diff`/`workingChanges` -> `uiStore.openDiff` against the session's
 *     repo, landing on the first changed path (mirrors
 *     `useAgentResultDiff.ts`'s `path` handling for a result's diff).
 *   - `manual` -> `unreachable`. A manual chat was never started from a real
 *     item; the honest answer is "there is nothing to open," not a broken
 *     link (matches `agentDeskTargets.ts`'s stance on unbuilt destinations).
 */
export type SourceNavAction =
  | { kind: 'github'; itemKind: 'issue' | 'pr'; number: number }
  | { kind: 'openspecChange'; changeId: string }
  | { kind: 'commit'; oid: string }
  | { kind: 'diff'; path: string | null }
  | { kind: 'unreachable'; reason: string }

export function resolveSourceNav(source: SessionSource): SourceNavAction {
  switch (source.kind) {
    case 'manual':
      return { kind: 'unreachable', reason: 'This chat was not started from an issue, PR, or task.' }
    case 'issue':
      return { kind: 'github', itemKind: 'issue', number: source.number }
    case 'pullRequest':
      return { kind: 'github', itemKind: 'pr', number: source.number }
    case 'openSpecChange':
      return { kind: 'openspecChange', changeId: source.changeId }
    case 'openSpecTask':
      return { kind: 'openspecChange', changeId: source.changeId }
    case 'commit':
      return { kind: 'commit', oid: source.oid }
    case 'diff':
      return { kind: 'diff', path: source.paths[0] ?? null }
    case 'workingChanges':
      return { kind: 'diff', path: source.paths[0] ?? null }
    case 'imported':
      // The original lives in another application, which GitWyrm does not
      // launch. The picker's wording is honest about that,
      // and it is not a navigation this window can perform.
      return {
        kind: 'unreachable',
        reason: 'This chat was copied from another app. Open it there to see the original.',
      }
    case 'checkFailure':
      return {
        kind: 'unreachable',
        reason: 'Opening a failed check from a chat is not available in this window yet.',
      }
  }
}
