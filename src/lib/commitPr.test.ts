import { describe, expect, it } from 'vitest'
import type { CommitEntry, PrSummary, RefInfo } from '@/lib/bindings'
import { matchCommitToPr, matchExplanation, prNumberFromMessage } from './commitPr'

function commit(over: Partial<CommitEntry> = {}): CommitEntry {
  return {
    sha: 'a'.repeat(40),
    short_sha: 'aaaaaaa',
    summary: 'improved: tidy the graph',
    files_changed: null,
    additions: null,
    deletions: null,
    author_name: 'Ada',
    author_email: 'ada@example.com',
    author_initials: 'A',
    time: 0,
    lane: 0,
    parent_lanes: [],
    parent_shas: [],
    is_merge: false,
    refs: [],
    spec_id: null,
    ...over,
  } as CommitEntry
}

function pr(over: Partial<PrSummary> = {}): PrSummary {
  return {
    number: 218,
    title: 'Add merge banner',
    author: 'ada',
    author_is_bot: false,
    draft: false,
    head_ref: 'merge-banner',
    base_ref: 'main',
    updated_at: null,
    created_at: '2026-01-01T00:00:00Z',
    html_url: 'https://github.com/o/r/pull/218',
    ...over,
  } as PrSummary
}

const ref = (name: string, type: RefInfo['type']): RefInfo => ({ name, type })

describe('prNumberFromMessage', () => {
  it('reads a merge commit summary', () => {
    expect(prNumberFromMessage('Merge pull request #218 from o/merge-banner')).toEqual({
      number: 218,
      reason: 'merge',
    })
  })

  it('reads a squashed commit summary', () => {
    expect(prNumberFromMessage('new: Add merge banner (#218)')).toEqual({
      number: 218,
      reason: 'squash',
    })
  })

  it('ignores an issue reference in the middle of a summary', () => {
    expect(prNumberFromMessage('fixes: crash on open, follows #204 work')).toBeNull()
  })

  it('ignores a mention of merging that is not the GitHub phrase', () => {
    expect(prNumberFromMessage('improved: describe how to merge pull request branches')).toBeNull()
  })
})

describe('matchCommitToPr', () => {
  it('matches a merge commit to the open pull request it names', () => {
    const m = matchCommitToPr(commit({ summary: 'Merge pull request #218 from o/x' }), [pr()])
    expect(m).toMatchObject({ number: 218, title: 'Add merge banner', reason: 'merge' })
  })

  it('still reports the number when the pull request is no longer open', () => {
    const m = matchCommitToPr(commit({ summary: 'new: thing (#77)' }), [pr()])
    expect(m).toMatchObject({ number: 77, webUrl: '', reason: 'squash' })
  })

  it('matches an ordinary commit by sha through the fetched commit lists', () => {
    const c = commit({ sha: 'b'.repeat(40) })
    const m = matchCommitToPr(c, [pr()], new Map([[c.sha, 218]]))
    expect(m).toMatchObject({ number: 218, reason: 'commit' })
  })

  it('matches a branch tip to the pull request built from it', () => {
    const c = commit({ refs: [ref('merge-banner', 'branch')] })
    expect(matchCommitToPr(c, [pr()])).toMatchObject({ number: 218, reason: 'branch' })
  })

  it('strips the remote prefix when matching a remote-tracking tip', () => {
    const c = commit({ refs: [ref('origin/merge-banner', 'remote')] })
    expect(matchCommitToPr(c, [pr()])).toMatchObject({ number: 218, reason: 'branch' })
  })

  it('prefers the message over the sha list when both point somewhere', () => {
    const c = commit({ sha: 'c'.repeat(40), summary: 'Merge pull request #218 from o/x' })
    const m = matchCommitToPr(c, [pr(), pr({ number: 9, title: 'Other', head_ref: 'other' })], new Map([[c.sha, 9]]))
    expect(m).toMatchObject({ number: 218, reason: 'merge' })
  })

  it('prefers the sha list over a branch tip', () => {
    const c = commit({ sha: 'd'.repeat(40), refs: [ref('merge-banner', 'branch')] })
    const prs = [pr(), pr({ number: 9, title: 'Other', head_ref: 'other' })]
    const m = matchCommitToPr(c, prs, new Map([[c.sha, 9]]))
    expect(m).toMatchObject({ number: 9, reason: 'commit' })
  })

  it('returns null for a commit with no tie to any pull request', () => {
    expect(matchCommitToPr(commit(), [pr()])).toBeNull()
  })

  it('does not match a tag that happens to share the branch name', () => {
    const c = commit({ refs: [ref('merge-banner', 'tag')] })
    expect(matchCommitToPr(c, [pr()])).toBeNull()
  })
})

describe('matchExplanation', () => {
  it('says something plain for each reason', () => {
    const base = { number: 218, title: 't', webUrl: '', draft: false } as const
    expect(matchExplanation({ ...base, reason: 'merge' })).toContain('merged pull request #218')
    expect(matchExplanation({ ...base, reason: 'commit' })).toContain('part of pull request #218')
    expect(matchExplanation({ ...base, reason: 'branch' })).toContain('latest on the branch')
  })
})
