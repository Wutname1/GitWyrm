import { describe, expect, it } from 'vitest'
import type {
  CompleteOpenSpecTaskOutcome,
  EscalateToFixOutcome,
  ResultChangedPath,
  ResultCheckOutcome,
  ResultRecord,
  SessionState,
  StartExecutionOutcome,
  ToggleOutcome,
} from '@/lib/bindings'
import {
  canEscalateToFix,
  changedPathStatusLabel,
  changedPathsSummaryLine,
  describeCommitDestination,
  explainCleanupOutcome,
  failingCheckLines,
  runActivityLabel,
  runIsActive,
  formatDiskSize,
  summarizeDiskUsage,
  checksSummaryLine,
  explainAutoStartOutcome,
  explainCommitOutcome,
  explainCompleteOpenSpecTaskOutcome,
  explainEscalateToFixOutcome,
  explainKeepOutcome,
  explainUndoOutcome,
  hasFailingCheck,
  resultActionAvailability,
  resultNeedsReview,
  resultStateLabel,
  shouldShowResultPanel,
  sortResultsNewestFirst,
  summarizeChangedPaths,
} from './agentDeskResult'

function path(status: string, p = 'a.ts'): ResultChangedPath {
  return { path: p, oldPath: null, status }
}

function record(overrides: Partial<ResultRecord> = {}): ResultRecord {
  return {
    executionId: 'exec-1',
    outcome: 'finished',
    state: 'reviewing',
    worktreePath: null,
    branch: null,
    baseOid: null,
    headOid: null,
    changedPaths: [],
    checks: [],
    commit: null,
    openspecChangeId: null,
    updatedAt: '2026-01-01T00:00:00Z',
    ...overrides,
  }
}

describe('resultStateLabel', () => {
  it('labels every state in plain language', () => {
    expect(resultStateLabel('reviewing')).toBe('Ready to review')
    expect(resultStateLabel('revisionRequested')).toBe('Revision requested')
    expect(resultStateLabel('kept')).toBe('Kept, not committed yet')
    expect(resultStateLabel('committed')).toBe('Committed')
    expect(resultStateLabel('discarded')).toBe('Discarded')
    expect(resultStateLabel('cleanupNeeded')).toBe('Needs cleanup')
    expect(resultStateLabel('cleanupFailed')).toBe('Cleanup failed')
  })
})

describe('resultNeedsReview', () => {
  it('is true only for reviewing/revisionRequested', () => {
    expect(resultNeedsReview('reviewing')).toBe(true)
    expect(resultNeedsReview('revisionRequested')).toBe(true)
    expect(resultNeedsReview('kept')).toBe(false)
    expect(resultNeedsReview('committed')).toBe(false)
    expect(resultNeedsReview('discarded')).toBe(false)
  })
})

describe('resultActionAvailability', () => {
  it('a read-only result (no worktree) offers nothing to land', () => {
    const r = record({ state: 'reviewing', worktreePath: null, changedPaths: [] })
    const a = resultActionAvailability(r)
    expect(a.canKeep).toBe(false)
    expect(a.canUndo).toBe(false)
    expect(a.canCommit).toBe(false)
    expect(a.canDraftPullRequest).toBe(false)
  })

  it('never offers to teach the spec from work that was not accepted', () => {
    // The spec is the project's source of truth and the one place a mistake
    // outlives the session, so writing back to it is a post-acceptance
    // gesture. This used to be ungated: you could Undo an agent's work and
    // then tell the spec what that discarded work proved.
    const wt = { worktreePath: 'C:/wt', changedPaths: [path('M')] }
    expect(resultActionAvailability({ state: 'reviewing', ...wt }).canTellSpec).toBe(false)
    expect(resultActionAvailability({ state: 'revisionRequested', ...wt }).canTellSpec).toBe(false)
    expect(resultActionAvailability({ state: 'discarded', ...wt }).canTellSpec).toBe(false)
  })

  it('offers to teach the spec once the work has been accepted', () => {
    const wt = { worktreePath: 'C:/wt', changedPaths: [path('M')] }
    expect(resultActionAvailability({ state: 'kept', ...wt }).canTellSpec).toBe(true)
    expect(resultActionAvailability({ state: 'committed', ...wt }).canTellSpec).toBe(true)
  })

  it('a reviewing result with changes can be kept and undone', () => {
    const r = record({ state: 'reviewing', worktreePath: 'C:/wt', changedPaths: [path('M')] })
    const a = resultActionAvailability(r)
    expect(a.canKeep).toBe(true)
    expect(a.canUndo).toBe(true)
    expect(a.canRequestRevision).toBe(true)
    expect(a.canCommit).toBe(false)
  })

  it('a kept result can be committed but not kept again', () => {
    const r = record({ state: 'kept', worktreePath: 'C:/wt', changedPaths: [path('M')] })
    const a = resultActionAvailability(r)
    expect(a.canKeep).toBe(false)
    expect(a.canCommit).toBe(true)
  })

  it('a committed result can draft a PR and be cleaned up', () => {
    const r = record({ state: 'committed', worktreePath: 'C:/wt', changedPaths: [] })
    const a = resultActionAvailability(r)
    expect(a.canDraftPullRequest).toBe(true)
    expect(a.canCleanup).toBe(true)
    expect(a.canCommit).toBe(false)
  })

  it('a discarded result can be cleaned up but not committed or kept', () => {
    const r = record({ state: 'discarded', worktreePath: 'C:/wt', changedPaths: [] })
    const a = resultActionAvailability(r)
    expect(a.canCleanup).toBe(true)
    expect(a.canCommit).toBe(false)
    expect(a.canKeep).toBe(false)
  })
})

describe('summarizeChangedPaths', () => {
  it('counts each status independently', () => {
    const paths = [path('A'), path('M'), path('M'), path('D'), path('R'), path('!')]
    expect(summarizeChangedPaths(paths)).toEqual({ added: 1, modified: 2, deleted: 1, renamed: 1, conflicted: 1 })
  })

  it('an empty list is all zeros', () => {
    expect(summarizeChangedPaths([])).toEqual({ added: 0, modified: 0, deleted: 0, renamed: 0, conflicted: 0 })
  })
})

describe('changedPathsSummaryLine', () => {
  it('says no changes for an empty list', () => {
    expect(changedPathsSummaryLine([])).toBe('No file changes')
  })
  it('uses singular for exactly one file', () => {
    expect(changedPathsSummaryLine([path('M')])).toBe('1 file changed')
  })
  it('uses plural for more than one', () => {
    expect(changedPathsSummaryLine([path('M', 'a.ts'), path('M', 'b.ts')])).toBe('2 files changed')
  })
})

describe('changedPathStatusLabel', () => {
  // The review list is the user's evidence of what the agent did to their
  // files. It used to render the bare status code, so a deleted file was a
  // single grey "D" -- the scariest outcome shown as the quietest mark.
  it('names each outcome in words a person can read', () => {
    expect(changedPathStatusLabel('A').label).toBe('Added')
    expect(changedPathStatusLabel('M').label).toBe('Changed')
    expect(changedPathStatusLabel('D').label).toBe('Deleted')
    expect(changedPathStatusLabel('R').label).toBe('Renamed')
    expect(changedPathStatusLabel('!').label).toBe('Needs a fix')
  })

  it('colours a deletion and a conflict as removals, and an addition as added', () => {
    expect(changedPathStatusLabel('D').tone).toBe('removed')
    expect(changedPathStatusLabel('!').tone).toBe('removed')
    expect(changedPathStatusLabel('A').tone).toBe('added')
    expect(changedPathStatusLabel('M').tone).toBe('changed')
  })

  it('never leaks an unrecognised code back to the user', () => {
    // A future status code must read as something, not as a bare letter.
    expect(changedPathStatusLabel('Z').label).toBe('Changed')
    expect(changedPathStatusLabel('').label).toBe('Changed')
  })
})

describe('checksSummaryLine', () => {
  function check(outcome: ResultCheckOutcome['outcome']): ResultCheckOutcome {
    return { commandName: 'npm run test', outcome, summary: null }
  }
  it('is null for no checks', () => {
    expect(checksSummaryLine([])).toBeNull()
  })
  it('combines passed/failed/skipped counts', () => {
    expect(checksSummaryLine([check('passed'), check('passed'), check('failed')])).toBe('2 passed, 1 failed')
  })
  it('omits a zero-count bucket rather than showing "0 failed"', () => {
    const line = checksSummaryLine([check('passed')])
    expect(line).toBe('1 passed')
    expect(line).not.toContain('0')
  })
})

describe('hasFailingCheck', () => {
  it('is true when any check failed', () => {
    expect(hasFailingCheck([{ commandName: 'x', outcome: 'passed', summary: null }, { commandName: 'y', outcome: 'failed', summary: null }])).toBe(true)
  })
  it('is false when none failed', () => {
    expect(hasFailingCheck([{ commandName: 'x', outcome: 'passed', summary: null }])).toBe(false)
  })
})

describe('explainKeepOutcome', () => {
  it('a plain Kept has no explanation (the caller shows a positive confirmation)', () => {
    expect(explainKeepOutcome({ kind: 'kept', record: record({ state: 'kept' }) })).toBeNull()
  })
  it('nothingToKeep is explained in plain language', () => {
    expect(explainKeepOutcome({ kind: 'nothingToKeep' })).toContain('nothing to keep')
  })
})

describe('explainUndoOutcome', () => {
  it('a plain Discarded has no explanation and is not a hand-edit refusal', () => {
    const { message, refusedHandEdited } = explainUndoOutcome({ kind: 'discarded', record: record({ state: 'discarded' }) })
    expect(message).toBeNull()
    expect(refusedHandEdited).toBe(false)
  })
  it('refusedHandEdited explains the count and is flagged distinctly', () => {
    const { message, refusedHandEdited } = explainUndoOutcome({ kind: 'refusedHandEdited', record: record(), modified: 2, untracked: 1 })
    expect(refusedHandEdited).toBe(true)
    expect(message).toContain('3')
  })
  it('a single changed file uses singular wording', () => {
    const { message } = explainUndoOutcome({ kind: 'refusedHandEdited', record: record(), modified: 1, untracked: 0 })
    expect(message).toContain('1 file')
    expect(message).not.toContain('1 files')
  })

  it('says nothing was thrown away, and never says "worktree"', () => {
    // The refusal is the safe outcome, so it has to read like one -- and this
    // was the only place that word reached a user, in an error, about a thing
    // they had never been shown.
    const { message } = explainUndoOutcome({ kind: 'refusedHandEdited', record: record(), modified: 2, untracked: 0 })
    expect(message).toContain('Nothing was thrown away')
    expect(message).not.toMatch(/worktree/i)
  })

  it('calls out new files, which are the ones with no way back', () => {
    const { message } = explainUndoOutcome({ kind: 'refusedHandEdited', record: record(), modified: 0, untracked: 1 })
    expect(message).toMatch(/1 of them is new/)
    expect(message).toMatch(/no saved version/)
  })
})

describe('explainCommitOutcome', () => {
  it('a plain Committed has no explanation', () => {
    expect(explainCommitOutcome({ kind: 'committed', record: record({ state: 'committed' }), oid: 'abc' })).toBeNull()
  })
  it('readOnlyIntent is explained without jargon', () => {
    const msg = explainCommitOutcome({ kind: 'readOnlyIntent' })
    expect(msg).toContain('cannot make changes')
  })
  it('gitFailed surfaces the underlying detail (may include a signing hint)', () => {
    const msg = explainCommitOutcome({ kind: 'gitFailed', detail: 'The signing key this repository uses is missing.' })
    expect(msg).toContain('signing key')
  })
})

describe('sortResultsNewestFirst', () => {
  it('orders by updatedAt descending', () => {
    const a = record({ executionId: 'a', updatedAt: '2026-01-01T00:00:00Z' })
    const b = record({ executionId: 'b', updatedAt: '2026-01-03T00:00:00Z' })
    const c = record({ executionId: 'c', updatedAt: '2026-01-02T00:00:00Z' })
    const sorted = sortResultsNewestFirst([a, b, c])
    expect(sorted.map((r) => r.executionId)).toEqual(['b', 'c', 'a'])
  })
})

// -- R3.8: shouldShowResultPanel -- ResultReviewPanel had zero importers
// anywhere in the app before this; these tests pin down exactly when its one
// production entry point (ConversationPane) decides to mount it.
describe('shouldShowResultPanel', () => {
  const terminal: SessionState[] = ['finished', 'failed', 'stopped']
  const nonTerminal: SessionState[] = ['draft', 'preparing', 'ready', 'working', 'needsInput', 'missingSource', 'interrupted']

  it.each(terminal)('shows the panel once the session has %s with an active execution', (state) => {
    expect(shouldShowResultPanel(state, 'exec-1')).toBe(true)
  })

  it.each(nonTerminal)('hides the panel while the session is %s, even with an active execution', (state) => {
    expect(shouldShowResultPanel(state, 'exec-1')).toBe(false)
  })

  it('hides the panel when there is no active execution at all, regardless of state', () => {
    expect(shouldShowResultPanel('finished', null)).toBe(false)
  })
})

// -- R3.1/R3.2: explainAutoStartOutcome -- pins the toast text an automatic
// Fix kickoff shows for every way `agentSessionStartExecution` can refuse,
// so the mapping is exercised without driving the actual composer/kickoff
// flow through a DOM this project's vitest setup cannot render.
describe('explainAutoStartOutcome', () => {
  // `started`/`alreadyRunning` only ever branch on `.kind` inside
  // `explainAutoStartOutcome` -- their other fields (`session`/`executionId`)
  // are irrelevant to what this function decides, so a minimal cast avoids
  // pinning this test to `bindings.ts`'s exact field casing for those
  // variants (which is inconsistent between generated types in this
  // codebase; not something this test should be coupled to).
  it('started needs no explanation -- the caller shows a positive confirmation instead', () => {
    expect(explainAutoStartOutcome({ kind: 'started' } as unknown as StartExecutionOutcome)).toBeNull()
  })

  it('alreadyRunning needs no explanation -- it is the same visible state a fresh start would have produced', () => {
    expect(explainAutoStartOutcome({ kind: 'alreadyRunning' } as unknown as StartExecutionOutcome)).toBeNull()
  })

  it('worktreeFailed names the isolation failure, matching R3.2\'s "worktree provisioning" gate', () => {
    const msg = explainAutoStartOutcome({ kind: 'worktreeFailed', detail: 'disk full' })
    expect(msg).toContain('isolated workspace')
    expect(msg).toContain('disk full')
  })

  it('every refusal kind produces non-empty, distinct text', () => {
    const outcomes: StartExecutionOutcome[] = [
      { kind: 'sourceMissing', detail: 'repo not open' },
      { kind: 'adapterUnsupported', detail: 'no CLI' },
      { kind: 'providerReconnect', detail: 'token expired' },
      { kind: 'notFound' },
      { kind: 'damaged', reason: 'bad json' },
      { kind: 'unavailable', detail: 'locked' },
      { kind: 'writeFailed', detail: 'disk error' },
    ]
    const messages = outcomes.map((o) => explainAutoStartOutcome(o))
    for (const m of messages) {
      expect(m).toBeTruthy()
    }
    expect(new Set(messages).size).toBe(messages.length)
  })
})

// -- P1-C wiring 1: explainCompleteOpenSpecTaskOutcome -- the plain-language
// half of "accepted task completion ticks the exact task through the
// existing writer." `ResultReviewPanel.handleKeep` calls
// `commands.agentSessionCompleteOpenspecTask` and this function whenever
// `isOpenSpecTask` is true; this pins that mapping without driving the
// panel's own DOM/query-client plumbing.
describe('explainCompleteOpenSpecTaskOutcome', () => {
  // `session` is never read by `explainCompleteOpenSpecTaskOutcome` (it only
  // branches on `.kind`/`.toggle`), so a minimal cast avoids pinning this
  // test to a full `AgentSession` fixture -- same reasoning
  // `explainAutoStartOutcome`'s own tests give for the same pattern above.
  const completed = (toggle: ToggleOutcome): CompleteOpenSpecTaskOutcome =>
    ({ kind: 'completed', session: {}, toggle }) as unknown as CompleteOpenSpecTaskOutcome

  it('toggled and alreadyThatWay both need no extra explanation -- the ordinary Keep toast already covers success', () => {
    expect(explainCompleteOpenSpecTaskOutcome(completed('toggled'))).toBeNull()
    expect(explainCompleteOpenSpecTaskOutcome(completed('alreadyThatWay'))).toBeNull()
  })

  it('lineMoved still explains itself even though the session write succeeded -- the checkbox itself may be wrong', () => {
    const msg = explainCompleteOpenSpecTaskOutcome(completed('lineMoved'))
    expect(msg).toBeTruthy()
    expect(msg).toContain('tasks.md')
  })

  it('notAnOpenSpecTaskSource is silent -- ResultReviewPanel is never supposed to call this for such a session', () => {
    expect(explainCompleteOpenSpecTaskOutcome({ kind: 'notAnOpenSpecTaskSource' })).toBeNull()
  })

  it('every real failure kind produces non-empty, distinct text', () => {
    const outcomes: CompleteOpenSpecTaskOutcome[] = [
      { kind: 'repoNotOpen' },
      { kind: 'noOpenSpecFolder' },
      { kind: 'sessionNotFound' },
      { kind: 'sessionDamaged', reason: 'bad json' },
      { kind: 'sessionUnavailable', detail: 'locked' },
      { kind: 'writeFailed', detail: 'disk error' },
    ]
    const messages = outcomes.map((o) => explainCompleteOpenSpecTaskOutcome(o))
    for (const m of messages) {
      expect(m).toBeTruthy()
    }
    expect(new Set(messages).size).toBe(messages.length)
  })
})

describe('canEscalateToFix', () => {
  it('offers Fix this only for the read-only intents', () => {
    expect(canEscalateToFix('ask')).toBe(true)
    expect(canEscalateToFix('explain')).toBe(true)
    expect(canEscalateToFix('review')).toBe(true)
    expect(canEscalateToFix('summarize')).toBe(true)
    expect(canEscalateToFix('fix')).toBe(false)
    expect(canEscalateToFix('plan')).toBe(false)
  })
})

describe('explainEscalateToFixOutcome', () => {
  it('is silent for created and plain-language for every refusal', () => {
    const session = {} as Extract<EscalateToFixOutcome, { kind: 'created' }>['session']
    expect(explainEscalateToFixOutcome({ kind: 'created', session })).toBeNull()
    expect(explainEscalateToFixOutcome({ kind: 'notFound' })).toBe('That chat could not be found.')
    expect(explainEscalateToFixOutcome({ kind: 'notAReview', intent: 'fix' })).toContain('already make changes')
    expect(explainEscalateToFixOutcome({ kind: 'nothingToFix', detail: 'The review has not said anything yet.' })).toBe(
      'The review has not said anything yet.'
    )
    expect(explainEscalateToFixOutcome({ kind: 'failed', detail: 'disk full' })).toBe('Could not start a fix chat: disk full')
  })
})

describe('describeCommitDestination', () => {
  // The panel showed changed files and checks and never said where a commit
  // lands. The backend commits to the WORKTREE's own HEAD, not the branch in
  // the main window, so silence here let a person believe the opposite.
  it('names the branch and says the open branch is untouched', () => {
    const d = describeCommitDestination({ worktreePath: 'C:/wt', branch: 'agent/fix-login' })
    expect(d?.branch).toBe('agent/fix-login')
    expect(d?.sentence).toMatch(/agent\/fix-login/)
    expect(d?.sentence).toMatch(/not touched/i)
  })

  it('still promises isolation when the branch name is unknown', () => {
    const d = describeCommitDestination({ worktreePath: 'C:/wt', branch: null })
    expect(d?.branch).toBeNull()
    expect(d?.sentence).toMatch(/separate copy/i)
  })

  it('promises nothing when there is no worktree to commit from', () => {
    expect(describeCommitDestination({ worktreePath: null, branch: 'main' })).toBeNull()
  })
})

describe('failingCheckLines', () => {
  const check = (name: string, outcome: ResultCheckOutcome['outcome'], summary: string | null = null) =>
    ({ commandName: name, outcome, summary }) as ResultCheckOutcome

  it('names what failed, which the aggregate count threw away', () => {
    const lines = failingCheckLines([
      check('npm run typecheck', 'failed', '3 errors'),
      check('npm test', 'passed'),
    ])
    expect(lines).toEqual(['npm run typecheck — 3 errors'])
  })

  it('still names a failure that reported no detail', () => {
    expect(failingCheckLines([check('cargo test', 'failed')])).toEqual(['cargo test failed'])
  })

  it('is empty when nothing failed', () => {
    expect(failingCheckLines([check('npm test', 'passed'), check('lint', 'skipped')])).toEqual([])
  })
})

describe('explainCleanupOutcome', () => {
  const rec = { record: {} as never }

  it('only reports removed when the space was actually reclaimed', () => {
    expect(explainCleanupOutcome({ kind: 'removed', ...rec }).removed).toBe(true)
    expect(explainCleanupOutcome({ kind: 'nothingToClean' }).removed).toBe(false)
    expect(explainCleanupOutcome({ kind: 'notIntegratedOrDiscarded' }).removed).toBe(false)
  })

  it('says what is in the way rather than just refusing', () => {
    const kept = explainCleanupOutcome({ kind: 'keptHandEdited', ...rec, modified: 2, untracked: 1 })
    expect(kept.removed).toBe(false)
    expect(kept.message).toMatch(/3 files/)
    expect(kept.message).toMatch(/not accounted for/i)
  })

  it('tells the person to land or discard the work first, not that something broke', () => {
    // The backend refuses cleanup before Commit or Undo on purpose; the copy
    // has to read as a safeguard, not a failure.
    expect(explainCleanupOutcome({ kind: 'notIntegratedOrDiscarded' }).message).toMatch(/so nothing is lost/i)
  })

  it('names the path when only part of it could go', () => {
    const partial = explainCleanupOutcome({ kind: 'partiallyRemoved', path: 'C:/wt/leftover' })
    expect(partial.message).toMatch(/C:\/wt\/leftover/)
    expect(partial.removed).toBe(false)
  })
})

describe('formatDiskSize', () => {
  it('scales to a unit a person can read', () => {
    expect(formatDiskSize(512)).toBe('512 B')
    expect(formatDiskSize(2048)).toBe('2.0 KB')
    expect(formatDiskSize(5 * 1024 * 1024)).toBe('5.0 MB')
    expect(formatDiskSize(1.4 * 1024 * 1024 * 1024)).toBe('1.4 GB')
  })

  it('drops the decimal once the number is big enough not to need it', () => {
    expect(formatDiskSize(12 * 1024 * 1024 * 1024)).toBe('12 GB')
  })

  it('says unknown rather than zero when it could not be measured', () => {
    // A copy we could not measure must not look like one that costs nothing.
    expect(formatDiskSize(null)).toBe('size unknown')
    expect(formatDiskSize(0)).toBe('0 B')
  })
})

describe('summarizeDiskUsage', () => {
  it('adds up what is there', () => {
    const copies = [{ sizeBytes: 1024 * 1024 * 100 }, { sizeBytes: 1024 * 1024 * 400 }]
    expect(summarizeDiskUsage(copies)).toBe('2 agent copies using 500 MB.')
  })

  it('uses the singular for one', () => {
    expect(summarizeDiskUsage([{ sizeBytes: 1024 * 1024 }])).toMatch(/^1 agent copy using/)
  })

  it('says how many it could not measure instead of quietly leaving them out', () => {
    // The total must never be smaller than the truth without saying so.
    const line = summarizeDiskUsage([{ sizeBytes: 1024 * 1024 }, { sizeBytes: null }])
    expect(line).toMatch(/2 agent copies/)
    expect(line).toMatch(/1 could not be measured/)
  })

  it('does not invent a total when nothing could be measured', () => {
    expect(summarizeDiskUsage([{ sizeBytes: null }])).toBe('1 agent copy, size unknown.')
  })

  it('says plainly when there are none', () => {
    expect(summarizeDiskUsage([])).toBe('No agent copies on disk.')
  })
})

describe('runIsActive', () => {
  // This rule was written by hand in four places and one copy left out
  // `needsInput`, so the transcript showed nothing while an agent waited for
  // an answer -- the composer offered Stop, the graph offered Stop, and the
  // pane the person was reading looked idle.
  it('counts a chat waiting on the person as still going', () => {
    expect(runIsActive('needsInput')).toBe(true)
    expect(runIsActive('working')).toBe(true)
    expect(runIsActive('preparing')).toBe(true)
  })

  it('is false for every state where nothing is running', () => {
    for (const state of ['draft', 'ready', 'finished', 'failed', 'stopped', 'interrupted', 'missingSource'] as const) {
      expect(runIsActive(state)).toBe(false)
    }
  })

  it('treats an unknown state as not running rather than throwing', () => {
    expect(runIsActive(null)).toBe(false)
    expect(runIsActive(undefined)).toBe(false)
  })
})

describe('runActivityLabel', () => {
  it('says waiting rather than working when the agent needs an answer', () => {
    // "Working…" beside a pulse tells someone to sit tight, which is the
    // wrong thing to say when their answer is the only thing missing.
    expect(runActivityLabel('needsInput')).toBe('Waiting for your answer…')
    expect(runActivityLabel('working')).toBe('Working…')
    expect(runActivityLabel('preparing')).toBe('Getting ready…')
  })

  it('says nothing for a chat that is not running', () => {
    expect(runActivityLabel('finished')).toBeNull()
    expect(runActivityLabel(null)).toBeNull()
  })

  it('has words for every state it claims to be active', () => {
    // The two must not disagree: an active state with no label would render
    // an empty status line.
    for (const state of ['working', 'preparing', 'needsInput'] as const) {
      expect(runIsActive(state)).toBe(true)
      expect(runActivityLabel(state)).not.toBeNull()
    }
  })
})
