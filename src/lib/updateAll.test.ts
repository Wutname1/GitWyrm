import { describe, expect, it } from 'vitest'
import type { BranchUpdate, RepoUpdate } from './bindings'
import {
  branchUpdateText,
  groupRepoUpdates,
  mergeRepoUpdates,
  updateDetail,
  updateHeadline,
  updateTotals,
} from './updateAll'

function repo(name: string, over: Partial<RepoUpdate> = {}): RepoUpdate {
  return {
    name,
    path: `C:\\code\\${name}`,
    level: 'unchanged',
    message: null,
    needs_sign_in: false,
    changes_in_the_way: false,
    commits_received: 0,
    branches: [],
    up_to_date: 0,
    skipped: false,
    ...over,
  }
}

function branch(name: string, kind: BranchUpdate['kind'], commits = 1): BranchUpdate {
  return { name, kind, commits, message: null }
}

describe('groupRepoUpdates', () => {
  /** The user asked for errors at the top, then warnings, then the rest. */
  it('puts errors first, then warnings, then updated, then unchanged', () => {
    const groups = groupRepoUpdates([
      repo('ok', { level: 'updated' }),
      repo('same'),
      repo('warn', { level: 'warning' }),
      repo('err', { level: 'error' }),
    ])
    expect(groups.map((g) => g.section)).toEqual(['error', 'warning', 'updated', 'unchanged'])
  })

  it('keeps projects a stopped run never reached apart from ones that were checked', () => {
    const groups = groupRepoUpdates([repo('late', { skipped: true }), repo('same')])
    expect(groups.map((g) => g.section)).toEqual(['unchanged', 'skipped'])
  })
})

describe('updateTotals and wording', () => {
  const results = [
    repo('api', {
      level: 'updated',
      commits_received: 5,
      branches: [branch('main', 'updated', 3), branch('develop', 'updated', 2)],
    }),
    repo('web', {
      level: 'warning',
      changes_in_the_way: true,
      branches: [branch('main', 'changes_in_the_way', 4)],
    }),
    repo('docs', { level: 'error', needs_sign_in: true, message: 'Sign-in needed' }),
  ]

  it('counts branches and commits across projects', () => {
    const totals = updateTotals(results)
    expect(totals.branchesUpdated).toBe(2)
    expect(totals.commits).toBe(5)
    expect(totals.updatedProjects).toBe(1)
    expect(totals.needsSignIn).toBe(1)
    expect(totals.changesInTheWay).toBe(1)
  })

  it('reads as plain sentences', () => {
    const totals = updateTotals(results)
    expect(updateHeadline(totals)).toBe('Updated 2 branches in 1 project')
    expect(updateDetail(totals, false)).toBe('5 new commits · 2 projects need a look')
  })

  it('talks about branches, not projects, inside one project', () => {
    const totals = updateTotals([results[1]])
    expect(updateDetail(totals, false, 'repo')).toBe('1 branch needs a look')
  })

  it('says when nothing changed', () => {
    expect(updateHeadline(updateTotals([repo('same')]))).toBe('Everything is already up to date')
  })

  it('explains a skipped checked-out branch in terms of the user\'s work', () => {
    expect(branchUpdateText(branch('main', 'changes_in_the_way', 4))).toContain('unsaved changes')
  })
})

describe('mergeRepoUpdates', () => {
  /** Retrying a couple of projects must not throw away the rest of the report. */
  it('replaces retried projects and keeps the others', () => {
    const before = [repo('web', { level: 'warning', changes_in_the_way: true }), repo('api', { level: 'updated' })]
    const retry = [repo('web', { level: 'updated', path: 'c:/code/web' })]
    const merged = mergeRepoUpdates(before, retry)
    expect(merged).toHaveLength(2)
    expect(merged.every((r) => r.level === 'updated')).toBe(true)
  })
})
