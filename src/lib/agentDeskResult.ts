import type {
  CommitResultOutcome,
  KeepResultOutcome,
  ResultCheckOutcome,
  ResultChangedPath,
  ResultRecord,
  ResultState,
  SessionState,
  StartExecutionOutcome,
  UndoResultOutcome,
} from '@/lib/bindings'

/**
 * Pure display/decision logic for the review/landing package
 * (agent-desk-review-and-landing). Kept out of the component so it is
 * testable under `vitest`'s Node environment (no DOM) -- see
 * `src/lib/agentSessionGrouping.ts` for the same pattern this project uses
 * throughout `src/lib/*.test.ts`.
 */

/** Plain-language label for a result's current state. */
export function resultStateLabel(state: ResultState): string {
  switch (state) {
    case 'reviewing':
      return 'Ready to review'
    case 'revisionRequested':
      return 'Revision requested'
    case 'kept':
      return 'Kept, not committed yet'
    case 'committed':
      return 'Committed'
    case 'discarded':
      return 'Discarded'
    case 'cleanupNeeded':
      return 'Needs cleanup'
    case 'cleanupFailed':
      return 'Cleanup failed'
  }
}

/** Whether a result still needs the user's attention in a review surface. */
export function resultNeedsReview(state: ResultState): boolean {
  return state === 'reviewing' || state === 'revisionRequested'
}

/**
 * What actions to offer for a result, given its current state and whether it
 * has any landable changes. Kept as one function so the review panel's
 * button row and any future surface (e.g. a graph node's context menu) never
 * disagree about which actions are valid right now.
 */
export interface ResultActionAvailability {
  canKeep: boolean
  canUndo: boolean
  canRequestRevision: boolean
  canCommit: boolean
  canDraftPullRequest: boolean
  canCleanup: boolean
}

export function resultActionAvailability(record: Pick<ResultRecord, 'state' | 'worktreePath' | 'changedPaths'>): ResultActionAvailability {
  const hasChanges = record.worktreePath != null && record.changedPaths.length > 0
  const reviewing = record.state === 'reviewing' || record.state === 'revisionRequested'
  return {
    canKeep: reviewing && hasChanges,
    canUndo: reviewing && record.worktreePath != null,
    canRequestRevision: reviewing,
    canCommit: record.state === 'kept',
    canDraftPullRequest: record.state === 'committed',
    canCleanup: record.state === 'committed' || record.state === 'discarded' || record.state === 'cleanupNeeded',
  }
}

/** One line per changed file, grouped by status for a compact summary. */
export function summarizeChangedPaths(paths: ResultChangedPath[]): { added: number; modified: number; deleted: number; renamed: number; conflicted: number } {
  const out = { added: 0, modified: 0, deleted: 0, renamed: 0, conflicted: 0 }
  for (const p of paths) {
    switch (p.status) {
      case 'A':
        out.added += 1
        break
      case 'M':
        out.modified += 1
        break
      case 'D':
        out.deleted += 1
        break
      case 'R':
        out.renamed += 1
        break
      case '!':
        out.conflicted += 1
        break
    }
  }
  return out
}

/** A short plain-language summary line, e.g. "3 files changed" / "no changes". */
export function changedPathsSummaryLine(paths: ResultChangedPath[]): string {
  if (paths.length === 0) return 'No file changes'
  if (paths.length === 1) return '1 file changed'
  return `${paths.length} files changed`
}

/** A short plain-language line for a check outcome list, e.g. "2 passed, 1 failed". */
export function checksSummaryLine(checks: ResultCheckOutcome[]): string | null {
  if (checks.length === 0) return null
  const passed = checks.filter((c) => c.outcome === 'passed').length
  const failed = checks.filter((c) => c.outcome === 'failed').length
  const skipped = checks.filter((c) => c.outcome === 'skipped').length
  const parts: string[] = []
  if (passed > 0) parts.push(`${passed} passed`)
  if (failed > 0) parts.push(`${failed} failed`)
  if (skipped > 0) parts.push(`${skipped} skipped`)
  return parts.join(', ')
}

/** Whether any check failed -- used to warn before Keep/Commit, never to block it. */
export function hasFailingCheck(checks: ResultCheckOutcome[]): boolean {
  return checks.some((c) => c.outcome === 'failed')
}

/**
 * Plain-language explanation for a Keep outcome that was not a plain
 * success, for a toast. `null` for `Kept` (the caller shows a positive
 * confirmation instead, not an explanation).
 */
export function explainKeepOutcome(outcome: KeepResultOutcome): string | null {
  switch (outcome.kind) {
    case 'kept':
      return null
    case 'nothingToKeep':
      return 'There is nothing to keep -- this result made no file changes.'
    case 'resultNotFound':
      return 'That result could not be found. Try refreshing.'
    case 'sessionNotFound':
      return 'That session could not be found.'
    case 'sessionDamaged':
      return `That session's file is damaged: ${outcome.reason}`
    case 'sessionUnavailable':
      return `That session could not be read right now: ${outcome.detail}`
    case 'writeFailed':
      return `Could not save: ${outcome.detail}`
  }
}

/** Same shape for Undo -- distinguishes the hand-edit refusal from a plain success. */
export function explainUndoOutcome(outcome: UndoResultOutcome): { message: string | null; refusedHandEdited: boolean } {
  switch (outcome.kind) {
    case 'discarded':
      return { message: null, refusedHandEdited: false }
    case 'refusedHandEdited': {
      const total = outcome.modified + outcome.untracked
      const noun = total === 1 ? 'change' : 'changes'
      return {
        message: `This worktree has ${total} hand-edited ${noun} since the agent finished. Open it to look, or discard by hand.`,
        refusedHandEdited: true,
      }
    }
    case 'nothingToUndo':
      return { message: 'There is nothing to undo -- this result made no file changes.', refusedHandEdited: false }
    case 'resultNotFound':
      return { message: 'That result could not be found. Try refreshing.', refusedHandEdited: false }
    case 'sessionNotFound':
      return { message: 'That session could not be found.', refusedHandEdited: false }
    case 'sessionDamaged':
      return { message: `That session's file is damaged: ${outcome.reason}`, refusedHandEdited: false }
    case 'sessionUnavailable':
      return { message: `That session could not be read right now: ${outcome.detail}`, refusedHandEdited: false }
    case 'writeFailed':
      return { message: `Could not save: ${outcome.detail}`, refusedHandEdited: false }
    case 'stateChanged':
      return {
        message: 'This result already changed somewhere else -- probably it was just committed. Refresh to see its current state.',
        refusedHandEdited: false,
      }
  }
}

/** Same shape for Commit. */
export function explainCommitOutcome(outcome: CommitResultOutcome): string | null {
  switch (outcome.kind) {
    case 'committed':
      return null
    case 'readOnlyIntent':
      return 'This chat cannot make changes, so there is nothing to commit.'
    case 'nothingToCommit':
      return 'Keep this result first, then commit it.'
    case 'messageRequired':
      return 'Write a commit message first.'
    case 'resultNotFound':
      return 'That result could not be found. Try refreshing.'
    case 'sessionNotFound':
      return 'That session could not be found.'
    case 'sessionDamaged':
      return `That session's file is damaged: ${outcome.reason}`
    case 'sessionUnavailable':
      return `That session could not be read right now: ${outcome.detail}`
    case 'writeFailed':
      return `Could not save: ${outcome.detail}`
    case 'gitFailed':
      return outcome.detail
    case 'recordStateChanged':
      return 'The commit was created, but this result changed at the same time (probably an Undo). Refresh and check whether the new commit needs to be reconciled by hand.'
  }
}

/** Sort results newest-first by `updatedAt`, for a review list. */
export function sortResultsNewestFirst(records: ResultRecord[]): ResultRecord[] {
  return [...records].sort((a, b) => b.updatedAt.localeCompare(a.updatedAt))
}

/**
 * R3.8: "Mount result review in the completed conversation." Whether
 * `ResultReviewPanel` should render below a conversation's transcript right
 * now.
 *
 * Deliberately keyed to `SessionState`, not `ResultState`: the result
 * sidecar may not have a record yet the instant a session finishes (`agent_result_build`
 * is a separate write from the state transition), and this function's job is
 * "does this conversation currently have a finished run worth reviewing,"
 * not "does a result record already exist" -- `ResultReviewPanel` itself
 * already handles "no result yet for this execution" as a loading/empty
 * state, so showing the panel a beat before its data lands is honest, not
 * broken.
 *
 * `Working`/`Preparing`/`NeedsInput`/`Draft`/`Ready` all say no: a review
 * surface for a run that has not stopped yet would invite Keep/Commit on
 * changes that could still be rewritten by the next tool call.
 */
export function shouldShowResultPanel(state: SessionState, activeExecutionId: string | null): boolean {
  if (activeExecutionId == null) return false
  return state === 'finished' || state === 'failed' || state === 'stopped'
}

/**
 * R3.2's auto-start toast, as plain-language explain-outcome text --
 * `null` for the two cases that need no user-facing explanation
 * (`started`, which gets a positive confirmation instead, and
 * `alreadyRunning`, which is invisible by design: it means a near-
 * simultaneous second kickoff found the engine already running, i.e.
 * exactly the state this call would have produced anyway).
 *
 * Pulled out of `useStartAgentSession` so the mapping from every
 * `StartExecutionOutcome` variant to its message is covered by a fast unit
 * test rather than only by clicking Fix in the app -- same reasoning as
 * `explainKeepOutcome`/`explainCommitOutcome`/`explainUndoOutcome` above.
 */
export function explainAutoStartOutcome(outcome: StartExecutionOutcome): string | null {
  switch (outcome.kind) {
    case 'started':
    case 'alreadyRunning':
      return null
    case 'worktreeFailed':
      return `Could not set up an isolated workspace to fix this in. ${outcome.detail}`
    case 'sourceMissing':
      return `This chat needs its repository open to run. ${outcome.detail}`
    case 'adapterUnsupported':
      return `That provider is not available right now. ${outcome.detail}`
    case 'providerReconnect':
      return `Reconnect the provider to continue. ${outcome.detail}`
    case 'notFound':
      return 'This chat is gone. It may have been archived elsewhere.'
    case 'damaged':
      return `This chat file is damaged and could not start. ${outcome.reason}`
    case 'unavailable':
      return `This chat could not be read right now. ${outcome.detail}`
    case 'writeFailed':
      return `Could not save: ${outcome.detail}`
    case 'unsupportedProvider':
      // R1.6: an override we cannot honour must say so, never quietly fall
      // back to a different provider and let the user believe their choice
      // was used.
      return `GitWyrm cannot run ${outcome.requested} yet, so nothing was started. Pick a different assistant and try again.`
  }
}
