import { describe, expect, it } from 'vitest'
import type { ResultChangedPath, ResultCheckOutcome, ResultRecord } from '@/lib/bindings'
import {
  changedPathsSummaryLine,
  checksSummaryLine,
  explainCommitOutcome,
  explainKeepOutcome,
  explainUndoOutcome,
  hasFailingCheck,
  resultActionAvailability,
  resultNeedsReview,
  resultStateLabel,
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
  it('a single hand-edited change uses singular wording', () => {
    const { message } = explainUndoOutcome({ kind: 'refusedHandEdited', record: record(), modified: 1, untracked: 0 })
    expect(message).toContain('1 hand-edited change')
    expect(message).not.toContain('1 hand-edited changes')
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
