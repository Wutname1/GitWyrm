import type {
  CommitResultOutcome,
  KeepResultOutcome,
  ResultCheckOutcome,
  ResultChangedPath,
  ResultRecord,
  ResultState,
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
  }
}

/** Sort results newest-first by `updatedAt`, for a review list. */
export function sortResultsNewestFirst(records: ResultRecord[]): ResultRecord[] {
  return [...records].sort((a, b) => b.updatedAt.localeCompare(a.updatedAt))
}
