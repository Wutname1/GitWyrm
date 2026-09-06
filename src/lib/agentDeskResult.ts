import type {
  CleanupWorktreeOutcome,
  StopExecutionOutcome,
  CommitResultOutcome,
  CompleteOpenSpecTaskOutcome,
  DraftPullRequestOutcome,
  EscalateToFixOutcome,
  KeepResultOutcome,
  ResultCheckOutcome,
  ResultChangedPath,
  ListResultsOutcome,
  ResultOutcomeKind,
  ResultRecord,
  ResultState,
  SessionIntent,
  UpdateSessionOutcome,
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
  /** Writing back to the spec is only offered once the work has been accepted. */
  canTellSpec: boolean
}

export function resultActionAvailability(record: Pick<ResultRecord, 'state' | 'worktreePath' | 'changedPaths'>): ResultActionAvailability {
  const hasChanges = record.worktreePath != null && record.changedPaths.length > 0
  // The named predicate above, not a second copy of it. Both existed; the
  // named one had no caller, which is how a rule ends up with two definitions
  // that can drift.
  const reviewing = resultNeedsReview(record.state)
  return {
    canKeep: reviewing && hasChanges,
    canUndo: reviewing && record.worktreePath != null,
    canRequestRevision: reviewing,
    canCommit: record.state === 'kept',
    canDraftPullRequest: record.state === 'committed',
    canCleanup: record.state === 'committed' || record.state === 'discarded' || record.state === 'cleanupNeeded',
    // Teaching the spec is a POST-acceptance gesture. Every other action here
    // gates on state; this one did not, so a result that was still being
    // reviewed -- or one the person had explicitly thrown away with Undo --
    // could still be written back into the spec as though it had proved
    // something. The spec is the project's source of truth, and it is the one
    // place a mistake outlives the session.
    canTellSpec: record.state === 'kept' || record.state === 'committed',
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

/**
 * Turns a raw status code into a word a person can read, plus the signal
 * colour that word should carry.
 *
 * The review list is the user's evidence of what the agent did to their
 * files, and it used to render the bare code -- so a deleted file was a
 * single grey `D`, the scariest outcome shown as the quietest mark. The
 * codes come from `crate::git::types::StatusCode` (`A | M | D | R | !`); an
 * unrecognised one falls back to "Changed" rather than leaking the letter.
 */
export function changedPathStatusLabel(status: string): { label: string; tone: 'added' | 'changed' | 'removed' | 'muted' } {
  switch (status) {
    case 'A':
      return { label: 'Added', tone: 'added' }
    case 'D':
      return { label: 'Deleted', tone: 'removed' }
    case 'R':
      return { label: 'Renamed', tone: 'muted' }
    case '!':
      return { label: 'Needs a fix', tone: 'removed' }
    case 'M':
    default:
      return { label: 'Changed', tone: 'changed' }
  }
}

/**
 * A short plain-language summary of what the agent did to the user's files.
 *
 * This is the headline on the review panel, and it used to say only how many
 * files changed. Twelve deleted files and twelve added ones both read
 * "12 files changed" -- the outcome a person most wants warned about was the
 * one the summary hid. The per-file marks do carry it, but only for the first
 * 50 rows, so on a large change the deletions sat behind "...and N more".
 *
 * Deletions and conflicts lead because they are what someone needs to know
 * before deciding to keep the work. A plain edit stays plain: an all-modified
 * change still reads "12 files changed" rather than growing a breakdown that
 * says nothing.
 */
export function changedPathsSummaryLine(paths: ResultChangedPath[]): string {
  if (paths.length === 0) return 'No file changes'
  const counts = summarizeChangedPaths(paths)
  const head = paths.length === 1 ? '1 file changed' : `${paths.length} files changed`
  const notable: string[] = []
  if (counts.deleted > 0) notable.push(`${counts.deleted} deleted`)
  if (counts.conflicted > 0) notable.push(`${counts.conflicted} with conflicts`)
  if (notable.length === 0) return head
  return `${head}, ${notable.join(' and ')}`
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
      return 'There is nothing to keep: this result made no file changes.'
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
      // Counts only what the agent did NOT leave, so this now names real
      // human edits rather than the agent's own output. "worktree" was also
      // the one place that word reached a user, in an error, about a thing
      // they had never been shown.
      const total = outcome.modified + outcome.untracked
      const noun = total === 1 ? 'file' : 'files'
      const newFiles =
        outcome.untracked > 0
          ? ` ${outcome.untracked} of them ${outcome.untracked === 1 ? 'is new, so it has' : 'are new, so they have'} no saved version to go back to.`
          : ''
      return {
        message: `${total} ${noun} in the agent's copy of your project changed after it finished. Nothing was thrown away.${newFiles} Open the folder to look, or remove them yourself first.`,
        refusedHandEdited: true,
      }
    }
    case 'nothingToUndo':
      return { message: 'There is nothing to undo: this result made no file changes.', refusedHandEdited: false }
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
        message: 'This result already changed somewhere else, probably because it was just committed. Refresh to see its current state.',
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

/**
 * P1-C wiring 1 ("the OpenSpec completion hook has no consumer"): plain-
 * language explanation for `commands.agentSessionCompleteOpenspecTask`'s
 * outcome, matching every other `explain*Outcome` helper's shape in this
 * file. Returns `null` for the two "the checkbox now reflects Done" cases
 * (`completed` with `toggle: 'toggled' | 'alreadyThatWay'`) -- a caller
 * reads `null` as "say nothing extra, the ordinary Keep success toast
 * already covers it." `completed` with `toggle: 'lineMoved'` still returns
 * a message: the SESSION write succeeded, but the actual checkbox in
 * `tasks.md` could not be located anymore (the file changed underneath the
 * session, `write::toggle_task_line`'s own guard) -- worth surfacing so the
 * user knows to double check the spec file by hand, distinct from a hard
 * failure.
 */
export function explainCompleteOpenSpecTaskOutcome(outcome: CompleteOpenSpecTaskOutcome): string | null {
  switch (outcome.kind) {
    case 'completed':
      if (outcome.toggle === 'lineMoved') {
        return 'Saved, but the task list changed since this chat started. Check tasks.md by hand to confirm the right item is checked off.'
      }
      // 'toggled' | 'alreadyThatWay': the ordinary Keep success toast
      // already covers it, nothing extra to say.
      return null
    case 'notAnOpenSpecTaskSource':
      // Not an error a user caused: this chat simply was not started from
      // an OpenSpec task, so there is nothing to check off. Callers should
      // not invoke this command for such a session in the first place (see
      // `ResultReviewPanel`'s `isOpenSpecTask` guard) -- this branch exists
      // so the switch is exhaustive, not because it is expected to fire.
      return null
    case 'repoNotOpen':
      return 'Open this project to update its task list.'
    case 'noOpenSpecFolder':
      return 'This project has no OpenSpec folder anymore, so the task list could not be updated.'
    case 'sessionNotFound':
      return 'That chat could not be found.'
    case 'sessionDamaged':
      return `That chat's file is damaged: ${outcome.reason}`
    case 'sessionUnavailable':
      return `That chat could not be read right now: ${outcome.detail}`
    case 'writeFailed':
      return `Could not update the task list: ${outcome.detail}`
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

/**
 * Source-kickoffs task 4.6 ("escalate a review into a fix"): whether a
 * finished chat of this intent should offer "Fix this". Exactly the
 * read-only intents -- a Fix or Plan chat can already make changes, so it
 * has nothing to escalate to. Mirrors the backend's `can_escalate_to_fix`
 * in `agent_kickoff.rs`; that check is the one that refuses, this one only
 * decides whether to draw the button.
 */
export function canEscalateToFix(intent: SessionIntent): boolean {
  return intent === 'ask' || intent === 'explain' || intent === 'review' || intent === 'summarize'
}

/**
 * Plain-language explanation for `commands.agentSessionEscalateToFix`'s
 * outcome, same shape as every other `explain*Outcome` helper here. `null`
 * for `created` (the caller shows a positive toast and opens the new chat).
 */
export function explainEscalateToFixOutcome(outcome: EscalateToFixOutcome): string | null {
  switch (outcome.kind) {
    case 'created':
      return null
    case 'notFound':
      return 'That chat could not be found.'
    case 'notAReview':
      return 'This chat can already make changes, so there is nothing to turn into a fix.'
    case 'nothingToFix':
      return outcome.detail
    case 'failed':
      return `Could not start a fix chat: ${outcome.detail}`
  }
}

/**
 * Where a commit from this result will actually land, in plain words.
 *
 * The review panel showed the changed files and the checks and never once
 * said which branch or which copy of the project the commit goes to. The
 * backend commits to the WORKTREE's own HEAD (`agent_result.rs:705,729`) --
 * a different branch from the one open in the main window -- so a person
 * pressing Commit could reasonably believe it was landing on their own
 * branch. "Where does this go" is the question an operator has to be able to
 * answer before an irreversible write.
 *
 * `null` when there is no worktree: nothing will be committed, so there is
 * no destination to promise.
 */
export function describeCommitDestination(
  record: Pick<ResultRecord, 'worktreePath' | 'branch'>
): { branch: string | null; isolated: boolean; sentence: string } | null {
  if (!record.worktreePath) return null
  const branch = record.branch ?? null
  return {
    branch,
    isolated: true,
    sentence: branch
      ? `Commits to ${branch}, in a separate copy of your project. The branch you have open is not touched.`
      : 'Commits into a separate copy of your project. The branch you have open is not touched.',
  }
}

/**
 * The checks that failed, named.
 *
 * The panel showed only the aggregate ("2 passed, 1 failed"), discarding the
 * `commandName` and `summary` each outcome already carries -- so at the one
 * moment the person decides whether to accept the work, they could see THAT
 * something failed but had to go elsewhere to learn WHAT. The name is the
 * part that decides whether a failure matters.
 */
export function failingCheckLines(checks: ResultCheckOutcome[]): string[] {
  return checks
    .filter((c) => c.outcome === 'failed')
    .map((c) => (c.summary ? `${c.commandName} — ${c.summary}` : `${c.commandName} failed`))
}

/**
 * Plain-language result of clearing away an agent's copy of the project, and
 * whether the space was actually reclaimed.
 *
 * `canCleanup` has been computed since results shipped and **no component
 * ever read it** -- the command was registered, bound, and unreachable. So
 * every agent run left a full checkout on disk with nothing in the app able
 * to remove it, and nothing that even said it was there.
 */
export function explainCleanupOutcome(outcome: CleanupWorktreeOutcome): { message: string; removed: boolean } {
  switch (outcome.kind) {
    case 'removed':
      return { message: "Cleared. The agent's copy of your project is gone from disk.", removed: true }
    case 'keptHandEdited': {
      const total = outcome.modified + outcome.untracked
      const noun = total === 1 ? 'file' : 'files'
      return {
        message: `Kept it: ${total} ${noun} in there are not accounted for. Open the folder to look before clearing it.`,
        removed: false,
      }
    }
    case 'notIntegratedOrDiscarded':
      return { message: 'Not yet: keep or throw away this work first, so nothing is lost.', removed: false }
    case 'nothingToClean':
      return { message: 'There is nothing to clear away for this run.', removed: false }
    case 'refusedLocked':
      return { message: `Something else is using that folder right now: ${outcome.path}`, removed: false }
    case 'partiallyRemoved':
      return { message: `Only part of it could be removed. What is left is at ${outcome.path}`, removed: false }
    case 'resultNotFound':
      return { message: 'That result could not be found. Try refreshing.', removed: false }
    case 'sessionNotFound':
      return { message: 'That chat could not be found.', removed: false }
    case 'sessionDamaged':
      return { message: `That chat's file is damaged: ${outcome.reason}`, removed: false }
    case 'sessionUnavailable':
      return { message: `That chat could not be read right now: ${outcome.detail}`, removed: false }
    case 'writeFailed':
      return { message: `Could not save the change: ${outcome.detail}`, removed: false }
  }
}

/**
 * A file size a person can read, or "unknown" when it could not be measured.
 *
 * Unknown deliberately does not fall back to 0: the usage panel already holds
 * the line that an unreported figure stays blank rather than reading as zero,
 * and a copy we could not measure must not look like a copy that costs
 * nothing.
 */
export function formatDiskSize(bytes: number | null): string {
  if (bytes == null) return 'size unknown'
  if (bytes < 1024) return `${Math.round(bytes)} B`
  const units = ['KB', 'MB', 'GB', 'TB']
  let value = bytes / 1024
  let unit = 0
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024
    unit += 1
  }
  // One decimal below 10 so "1.4 GB" and "12 GB" both read cleanly.
  return `${value < 10 ? value.toFixed(1) : Math.round(value)} ${units[unit]}`
}

/**
 * The headline for the agent-copies list: how much room they take together.
 *
 * Says how many could not be measured rather than quietly leaving them out of
 * the total, so the number is never smaller than the truth without saying so.
 */
export function summarizeDiskUsage(copies: Array<{ sizeBytes: number | null }>): string {
  if (copies.length === 0) return 'No agent copies on disk.'
  const measured = copies.filter((c) => c.sizeBytes != null)
  const total = measured.reduce((sum, c) => sum + (c.sizeBytes ?? 0), 0)
  const noun = copies.length === 1 ? 'copy' : 'copies'
  const unmeasured = copies.length - measured.length
  const tail = unmeasured > 0 ? ` (${unmeasured} could not be measured)` : ''
  if (measured.length === 0) return `${copies.length} agent ${noun}, size unknown.`
  return `${copies.length} agent ${noun} using ${formatDiskSize(total)}${tail}.`
}

/**
 * Whether a run is still going — the one predicate, in one place.
 *
 * This rule was written out by hand in four places and one copy left out
 * `needsInput`, so the transcript showed nothing at all while an agent waited
 * for an answer: the composer swapped Send for Stop and the graph offered
 * Stop, but the pane the person was actually reading looked idle at the exact
 * moment it most needed them. A user watching stillness concludes the run
 * died and presses Stop on work that was one answer away from finishing.
 *
 * The same shape of defect was fixed once before, for `failed` vs
 * `interrupted`, with a comment eleven lines from where it recurred here.
 * Fixing the instance did not stop the shape; a single exported predicate
 * does.
 */
export function runIsActive(state: SessionState | null | undefined): boolean {
  return state === 'working' || state === 'preparing' || state === 'needsInput'
}

/**
 * What a still-running chat should say it is doing.
 *
 * `needsInput` gets its own words rather than the "Working…" pulse: an agent
 * blocked on a person is not working, and saying so is the difference between
 * waiting patiently and giving up on a run.
 */
export function runActivityLabel(state: SessionState | null | undefined): string | null {
  switch (state) {
    case 'preparing':
      return 'Getting ready…'
    case 'working':
      return 'Working…'
    case 'needsInput':
      return 'Waiting for your answer…'
    default:
      return null
  }
}

/**
 * The badge treatment for a result state — exhaustive, so a new state cannot
 * ship unstyled.
 *
 * The chained-`&&` version this replaces had no branch for `cleanupNeeded`,
 * so the one state meaning "you still have something to do" rendered with no
 * background at all — reading as less urgent than "Discarded", which needs
 * nothing. A `switch` with no default means TypeScript fails the build when
 * the next variant lands, which is the guarantee the Rust side already gives
 * itself.
 */
export function resultStateTone(state: ResultState): string {
  switch (state) {
    case 'committed':
      return 'bg-success/15 text-success'
    case 'kept':
      return 'bg-accent/15 text-accent'
    case 'reviewing':
    case 'revisionRequested':
      return 'bg-muted text-muted-foreground'
    case 'cleanupNeeded':
      // Still asks something of the person, so it must not be quieter than a
      // state that asks nothing.
      return 'bg-[var(--gw-amber)]/15 text-[var(--gw-amber)]'
    case 'discarded':
    case 'cleanupFailed':
      return 'bg-destructive/15 text-destructive'
  }
}

/**
 * Plain-language result of stopping a chat's agents.
 *
 * Three call sites each branched on `stopped.length` alone and ignored
 * `timed_out` -- the list the backend documents as agents that were force
 * stopped without acknowledging. A hung agent produces `stopped: []` with
 * `timed_out: [id]`, so the person pressing Stop on a runaway agent was told
 * **"Nothing was running."** -- the opposite of the truth, at the moment they
 * most needed it, in a way that invites them to walk away from a live agent.
 *
 * A forced stop still preserves the work: the CLI process is killed either
 * way and the worktree is left exactly as an acknowledged stop leaves it, so
 * the message says that rather than implying anything was lost.
 */
export function explainStopOutcome(outcome: StopExecutionOutcome): { message: string; ok: boolean } {
  switch (outcome.kind) {
    case 'stopped': {
      const acked = outcome.stopped.length
      const forced = outcome.timed_out.length
      const plural = (n: number) => (n === 1 ? 'agent' : 'agents')
      if (acked === 0 && forced === 0) return { message: 'Nothing was running.', ok: true }
      if (forced === 0) {
        return { message: `Stopped ${acked} ${plural(acked)}; work already done was kept.`, ok: true }
      }
      if (acked === 0) {
        return {
          message: `Force-stopped ${forced} ${plural(forced)} that did not answer; work already done was kept.`,
          ok: true,
        }
      }
      return {
        message: `Stopped ${acked} ${plural(acked)}, and force-stopped ${forced} that did not answer; work already done was kept.`,
        ok: true,
      }
    }
    case 'notFound':
      return { message: 'This chat is gone. It may have been archived elsewhere.', ok: false }
    case 'damaged':
      return { message: `This chat's file is damaged and could not be stopped: ${outcome.reason}`, ok: false }
    case 'unavailable':
      return { message: `This chat could not be read right now: ${outcome.detail}`, ok: false }
    case 'writeFailed':
      return { message: `Could not save the change: ${outcome.detail}`, ok: false }
  }
}

/**
 * A last-resort description for an outcome with no explainer of its own.
 *
 * Ten toast call sites passed `outcome.kind` straight through as the
 * description, so a person read "providerReconnect" or "writeFailed" -- an
 * identifier meant for code, in the sentence meant to tell them what to do.
 *
 * This is the floor, not the goal: an outcome that matters enough to act on
 * deserves a real sentence in an `explain*` function, and where one already
 * exists the call site should use it. What this guarantees is that no path
 * shows the raw token. `writeFailed` becomes "write failed"; a variant
 * carrying its own `detail` should pass that instead, since the backend wrote
 * it for the person.
 */
const OUTCOME_KIND_TEXT: Record<string, string> = {
  // The kinds that actually reach this floor today. Title-casing alone still
  // handed over jargon: "Worktree failed" and "Adapter unsupported" are code
  // words, and the whole point of this function is that no code word reaches
  // a person. Anything not listed still falls back to the spaced form, so a
  // new variant degrades to readable rather than raw.
  notFound: 'It is no longer there.',
  damaged: 'Its saved file could not be read.',
  unavailable: 'GitWyrm could not read it right now.',
  sessionNotFound: 'That chat is no longer there.',
  sessionDamaged: "That chat's saved file could not be read.",
  sessionUnavailable: 'GitWyrm could not read that chat right now.',
  writeFailed: 'The change could not be saved.',
  executionNotFound: 'That agent is no longer part of this chat.',
  // Three `explain*` functions in this file already answer this one; only the
  // shared floor was missing it, so the one path that uses the floor --
  // marking a result for revision -- said "Result not found" instead.
  resultNotFound: 'That result could not be found. Try refreshing.',
  // Three `explain*` functions in this file already answer this one; only the
  // shared floor was missing it, so the one path that uses the floor --
  // marking a result for revision -- said "Result not found" instead.
  sourceMissing: 'The thing this chat was started from is gone.',
  adapterUnsupported: 'That chat app is not supported here.',
  providerReconnect: 'The AI tool needs to be connected again.',
  worktreeFailed: 'A working copy of your project could not be prepared.',
  alreadyRunning: 'It is already running.',
  concurrentChangeRefused: 'It changed after GitWyrm looked, so nothing was touched.',
  noProposal: 'There is no plan to act on.',
  noConflict: 'There is nothing to resolve.',
}

export function describeOutcomeKind(kind: string): string {
  const known = OUTCOME_KIND_TEXT[kind]
  if (known) return known
  const spaced = kind.replace(/([a-z0-9])([A-Z])/g, '$1 $2').toLowerCase()
  return spaced.charAt(0).toUpperCase() + spaced.slice(1)
}

/**
 * Why a pull request could not be prepared, in words that say what to do.
 *
 * Six distinct refusals collapsed into one "Could not prepare a pull request
 * for this result." Two of them are ordinary states the person can fix
 * themselves and were the two most likely to happen: `noCommit` means the work
 * has not been committed yet, and `noRemote` means the project has nowhere to
 * open a request against. Being told only that something did not work leaves
 * them re-clicking a button that will keep refusing for a reason nobody named.
 *
 * `drafted` is the success case and has no message; callers switch on it
 * before asking.
 */
export function explainDraftPullRequestRefusal(
  outcome: Exclude<DraftPullRequestOutcome, { kind: 'drafted' }>
): string {
  switch (outcome.kind) {
    case 'noCommit':
      return 'Commit this work first, then a pull request can be prepared from it.'
    case 'noRemote':
      return 'This project has no remote set up, so there is nowhere to open a pull request.'
    case 'resultNotFound':
      return 'That result is no longer there. It may have been undone or cleaned up.'
    case 'sessionNotFound':
      return 'That chat is no longer there.'
    case 'sessionDamaged':
      return `That chat's saved file could not be read: ${outcome.reason}`
    case 'sessionUnavailable':
      return `GitWyrm could not read that chat right now: ${outcome.detail}`
  }
}

/**
 * How a run ended, for the line above the file list.
 *
 * `ResultOutcomeKind` is persisted on every record and read by no component,
 * so a run that crashed, one the person stopped, and one that hit a conflict
 * all showed the same "Ready to review" badge as one that completed. Someone
 * coming back to a finished run could not tell which had happened.
 *
 * `finished` returns null: the state badge beside it already says the work is
 * ready, and repeating "it finished" adds nothing. The other three change what
 * the reader should expect from the file list below.
 */
export function runOutcomeLabel(outcome: ResultOutcomeKind): string | null {
  switch (outcome) {
    case 'finished':
      return null
    case 'stopped':
      return 'You stopped this run, so it may be part-way through.'
    case 'failed':
      return 'This run did not finish. What is here may be incomplete.'
    case 'conflicted':
      return 'This run hit a clash with other work and stopped there.'
  }
}

/**
 * The generic floor, plus whatever the backend actually said.
 *
 * `describeOutcomeKind` takes only the kind, so four inline call sites showed
 * "Its saved file could not be read." and threw away the `reason` sitting
 * beside it -- the one part written for this person about this failure. The
 * dedicated `explain*` helpers all interpolate it; the sites that skipped them
 * did not.
 *
 * Takes the whole outcome so the detail cannot be forgotten at the call site.
 * A variant with no detail is unchanged.
 */
export function describeOutcome(outcome: { kind: string; reason?: string; detail?: string }): string {
  const base = describeOutcomeKind(outcome.kind)
  const extra = outcome.reason ?? outcome.detail
  return extra ? `${base} ${extra}` : base
}

/**
 * Why the result list could not be read, when it could not.
 *
 * The panel collapsed `sessionDamaged`, `sessionUnavailable`, `sessionNotFound`
 * and every thrown error into an empty list, then rendered "No result yet for
 * this execution." -- a confident statement that the agent produced nothing,
 * on the screen where a person decides whether its work lands. Someone
 * returning to a run whose sidecar file has since become unreadable was told,
 * in plain words, that work they watched happen did not happen.
 *
 * `found` returns null: an empty list there genuinely means no result yet, and
 * the plain empty state says that better than an error would.
 */
export function explainResultListUnavailable(
  outcome: ListResultsOutcome | undefined,
  isError: boolean
): string | null {
  if (isError) return 'GitWyrm could not read this chat to find its results.'
  if (!outcome) return null
  switch (outcome.kind) {
    case 'found':
      return null
    case 'sessionNotFound':
      return 'That chat is no longer there, so its results cannot be shown.'
    case 'sessionDamaged':
      return `This chat's saved file could not be read, so its results are unknown: ${outcome.reason}`
    case 'sessionUnavailable':
      return `GitWyrm could not read this chat right now, so its results are unknown: ${outcome.detail}`
  }
}

/**
 * Whether a run ended in a way the person needs told about.
 *
 * This existed three times, hand-written, in three components -- and the copy
 * in the transcript (the one place someone is actually reading) omitted
 * `missingSource`, so a chat whose source could not be loaded showed a state
 * dot in the sidebar and the graph and no explanation at all in the
 * conversation itself.
 */
export function runStoppedBadly(state: SessionState | null | undefined): boolean {
  return state === 'failed' || state === 'interrupted' || state === 'missingSource'
}

/**
 * What to say about a run that stopped badly, or `null` when it did not.
 *
 * `missingSource` gets its own sentence: "stopped when the app closed" is
 * true for an interrupted run and simply wrong for a chat whose issue, pull
 * request or spec could not be read -- and the fix for the two is different,
 * so one sentence for both would send people to the wrong place.
 *
 * The same argument applies to `failed`, and used to stop one case short of
 * it. A run reaches `failed` two completely different ways: the agent itself
 * failed while GitWyrm watched (`bridge.rs`), or GitWyrm found it abandoned
 * after a crash and wrote that state itself (`session_recovery.rs`). Both
 * were told "This chat stopped when the app closed" -- a definite claim about
 * a cause GitWyrm did not observe, and one that points at restarting when
 * the actual answer is in what the agent said before it stopped.
 *
 * So `failed` now says the run failed without claiming why, and
 * `interrupted` keeps the sentence that is only true of it. Neither invents a
 * cause: the backend already separates these two states, and this is the one
 * place that was collapsing them back together.
 */
export function runStoppedBadlyLabel(state: SessionState | null | undefined): string | null {
  switch (state) {
    case 'missingSource':
      return 'GitWyrm could not open what this chat is about. It may have been moved, renamed or deleted.'
    case 'interrupted':
      return 'This chat stopped when the app closed. Send a message to start it again.'
    case 'failed':
      return 'This run did not finish. What the agent said above is the best clue why.'
    default:
      return null
  }
}

/**
 * Why saving a chat's provider/mode/team choice did not stick.
 *
 * Every non-success variant used to be turned into `new Error(outcome.kind)`
 * and handed to `describeError`, which returns `e.stack` -- so the toast
 * explained a failed save with a JavaScript stack trace, while the
 * `reason`/`detail` strings the backend sends for exactly this purpose were
 * discarded to build it.
 *
 * `notFound` deliberately carries no backend string: the chat is simply gone,
 * and there is nothing further to report about it.
 */
export function describeSetPreferencesFailure(outcome: UpdateSessionOutcome): string {
  switch (outcome.kind) {
    case 'updated':
      return ''
    case 'notFound':
      return 'This chat is no longer here.'
    case 'damaged':
      return outcome.reason
    case 'writeFailed':
      return outcome.detail
    case 'unavailable':
      return outcome.detail
  }
}
