import { describe, expect, it } from 'vitest'
import type { AgentSessionHeader, SessionSource, SessionState } from '@/lib/bindings'
import {
  buildSidebarRows,
  diffBucketLabel,
  formatCompactAge,
  normalizeRepoPath,
  recentBucket,
  resolveSessionRepoFilter,
  sourceKindLabel,
  summarizeAgentActivity,
} from './agentSessionGrouping'

// Anchored to local noon (not a fixed UTC instant) so day-boundary math in
// the assertions below is stable regardless of the test runner's timezone.
const NOW = (() => {
  const d = new Date()
  d.setHours(12, 0, 0, 0)
  return d.getTime()
})()
const DAY = 86_400_000

/** `daysAgo` full local calendar days before `NOW`, at the same time of day. */
function daysAgo(n: number): string {
  return new Date(NOW - n * DAY).toISOString()
}

function header(overrides: Partial<AgentSessionHeader> & { sessionId: string }): AgentSessionHeader {
  const source: SessionSource = { kind: 'manual', repoId: 'repo-1' }
  return {
    schemaVersion: 1,
    repoId: 'repo-1',
    repoPath: 'C:/code/repo',
    repoName: 'repo',
    title: 'Untitled',
    source,
    intent: 'ask',
    state: 'ready',
    createdAt: '2026-08-19T11:00:00Z',
    updatedAt: '2026-08-19T11:00:00Z',
    unread: false,
    changedFileCount: 0,
    activeExecutionId: null,
    archived: false,
    ...overrides,
  }
}

describe('recentBucket', () => {
  it('buckets same-day updates as Today', () => {
    expect(recentBucket(daysAgo(0), NOW)).toBe('Today')
  })

  it('buckets the previous calendar day as Yesterday', () => {
    expect(recentBucket(daysAgo(1), NOW)).toBe('Yesterday')
  })

  it('buckets 5 days ago as This week', () => {
    expect(recentBucket(daysAgo(5), NOW)).toBe('This week')
  })

  it('buckets anything older than a week as Earlier', () => {
    expect(recentBucket(daysAgo(45), NOW)).toBe('Earlier')
  })

  it('treats an unparseable timestamp as Earlier rather than throwing', () => {
    expect(recentBucket('not-a-date', NOW)).toBe('Earlier')
  })
})

describe('normalizeRepoPath', () => {
  it('treats backslash and forward-slash paths as equal', () => {
    expect(normalizeRepoPath('C:\\code\\Repo')).toBe(normalizeRepoPath('C:/code/Repo'))
  })

  it('is case-insensitive', () => {
    expect(normalizeRepoPath('C:/Code/Repo')).toBe(normalizeRepoPath('c:/code/repo'))
  })

  it('ignores a trailing slash', () => {
    expect(normalizeRepoPath('C:/code/repo/')).toBe(normalizeRepoPath('C:/code/repo'))
  })
})

describe('diffBucketLabel', () => {
  it('buckets sessions with no changed files separately from ones with changes', () => {
    expect(diffBucketLabel(header({ sessionId: 's1', changedFileCount: 0 }))).toBe('No changes yet')
  })

  it.each<[SessionState, string]>([
    ['working', 'Changing files'],
    ['needsInput', 'Needs your input'],
    ['finished', 'Ready to review'],
    ['failed', 'Failed'],
    ['stopped', 'Stopped with changes'],
  ])('labels a %s session with changes as %s', (state, expected) => {
    expect(diffBucketLabel(header({ sessionId: 's1', changedFileCount: 3, state }))).toBe(expected)
  })
})

describe('formatCompactAge', () => {
  it('formats sub-minute as now', () => {
    expect(formatCompactAge(new Date(NOW - 30_000).toISOString(), NOW)).toBe('now')
  })

  it('formats minutes', () => {
    expect(formatCompactAge(new Date(NOW - 9 * 60_000).toISOString(), NOW)).toBe('9m')
  })

  it('formats hours', () => {
    expect(formatCompactAge(new Date(NOW - 2 * 3_600_000).toISOString(), NOW)).toBe('2h')
  })

  it('formats days', () => {
    expect(formatCompactAge(new Date(NOW - 3 * 86_400_000).toISOString(), NOW)).toBe('3d')
  })

  it('falls back to a placeholder for an unparseable timestamp', () => {
    expect(formatCompactAge('garbage', NOW)).toBe('--')
  })
})

describe('sourceKindLabel', () => {
  // Typed, so leaving a kind out is a compile error rather than a quiet gap.
  // The old version claimed to cover "every SessionSource kind" while its own
  // list omitted `imported`, which is exactly the kind that was mislabelled.
  const ALL: SessionSource['kind'][] = [
    'manual',
    'issue',
    'pullRequest',
    'openSpecChange',
    'openSpecTask',
    'commit',
    'diff',
    'workingChanges',
    'checkFailure',
    'imported',
  ]

  it('maps every SessionSource kind to a label', () => {
    for (const kind of ALL) {
      expect(sourceKindLabel(kind)).not.toBe('')
    }
  })

  it('does not call an imported chat the same thing as one started here', () => {
    expect(sourceKindLabel('imported')).toBe('Imported chat')
    expect(sourceKindLabel('imported')).not.toBe(sourceKindLabel('manual'))
  })
})

describe('buildSidebarRows', () => {
  it('groups by day bucket in Recent mode, newest bucket first', () => {
    const headers = [
      header({ sessionId: 'today', updatedAt: daysAgo(0) }),
      header({ sessionId: 'yesterday', updatedAt: daysAgo(1) }),
    ]
    const rows = buildSidebarRows(headers, { mode: 'recent', collapsedGroupIds: new Set(), now: NOW })
    expect(rows.map((r) => r.id)).toEqual(['Today', 'today', 'Yesterday', 'yesterday'])
  })

  it('marks rows whose group header already names the project, and only those', () => {
    // The row shows the project so a chat cannot be mistaken for one in
    // another repository -- but repeating it under a group header that
    // already says it is noise, so grouping by project turns it off.
    const headers = [header({ sessionId: 's1', updatedAt: daysAgo(0) })]
    const byProject = buildSidebarRows(headers, { mode: 'project', collapsedGroupIds: new Set(), now: NOW })
    const byRecent = buildSidebarRows(headers, { mode: 'recent', collapsedGroupIds: new Set(), now: NOW })
    const flagOf = (rows: ReturnType<typeof buildSidebarRows>) => {
      const row = rows.find((r) => r.kind === 'session')
      return row?.kind === 'session' ? row.projectInHeader : undefined
    }
    expect(flagOf(byProject)).toBe(true)
    expect(flagOf(byRecent)).toBeFalsy()
  })

  it('keeps a group header when collapsed but omits its session rows', () => {
    const headers = [header({ sessionId: 's1', updatedAt: daysAgo(0) })]
    const rows = buildSidebarRows(headers, {
      mode: 'recent',
      collapsedGroupIds: new Set(['Today']),
      now: NOW,
    })
    expect(rows).toHaveLength(1)
    expect(rows[0]).toMatchObject({ kind: 'header', id: 'Today', collapsed: true, count: 1 })
  })

  it('groups Project mode by normalized path, not by name, so duplicate repo names stay distinct', () => {
    const headers = [
      header({ sessionId: 'a', repoName: 'repo', repoPath: 'C:/code/one/repo' }),
      header({ sessionId: 'b', repoName: 'repo', repoPath: 'C:/code/two/repo' }),
    ]
    const rows = buildSidebarRows(headers, { mode: 'project', collapsedGroupIds: new Set(), now: NOW })
    const headerRows = rows.filter((r) => r.kind === 'header')
    expect(headerRows).toHaveLength(2)
  })

  it('groups Project mode by path so slash/case variants of one repo do not duplicate', () => {
    const headers = [
      header({ sessionId: 'a', repoName: 'repo', repoPath: 'C:\\code\\repo' }),
      header({ sessionId: 'b', repoName: 'repo', repoPath: 'C:/code/repo/' }),
    ]
    const rows = buildSidebarRows(headers, { mode: 'project', collapsedGroupIds: new Set(), now: NOW })
    const headerRows = rows.filter((r) => r.kind === 'header')
    expect(headerRows).toHaveLength(1)
    expect(headerRows[0]).toMatchObject({ count: 2 })
  })

  it('falls back to the repo path as the Project label when repoName is blank', () => {
    const headers = [header({ sessionId: 'a', repoName: '  ', repoPath: 'C:/code/repo' })]
    const rows = buildSidebarRows(headers, { mode: 'project', collapsedGroupIds: new Set(), now: NOW })
    expect(rows[0]).toMatchObject({ kind: 'header', label: 'C:/code/repo' })
  })

  it('groups Diff mode by changed-file presence and state', () => {
    const headers = [
      header({ sessionId: 'clean', changedFileCount: 0 }),
      header({ sessionId: 'working', changedFileCount: 2, state: 'working' }),
    ]
    const rows = buildSidebarRows(headers, { mode: 'diff', collapsedGroupIds: new Set(), now: NOW })
    expect(rows.map((r) => r.id)).toEqual(['No changes yet', 'clean', 'Changing files', 'working'])
  })

  it('handles missing repo path by falling back to repoId as the grouping key', () => {
    const headers = [
      header({ sessionId: 'a', repoPath: '', repoId: 'repo-x', repoName: '' }),
      header({ sessionId: 'b', repoPath: '', repoId: 'repo-x', repoName: '' }),
    ]
    const rows = buildSidebarRows(headers, { mode: 'project', collapsedGroupIds: new Set(), now: NOW })
    const headerRows = rows.filter((r) => r.kind === 'header')
    expect(headerRows).toHaveLength(1)
    expect(headerRows[0]).toMatchObject({ count: 2 })
  })

  it('ellipsis-worthy long titles pass through unmodified -- truncation is a CSS concern, not a data concern', () => {
    const longTitle = 'A'.repeat(500)
    const headers = [header({ sessionId: 'a', title: longTitle })]
    const rows = buildSidebarRows(headers, { mode: 'recent', collapsedGroupIds: new Set(), now: NOW })
    const sessionRow = rows.find((r) => r.kind === 'session')
    expect(sessionRow?.kind === 'session' && sessionRow.header.title).toBe(longTitle)
  })

  it('handles 1,000 sessions without losing or duplicating any row, across all three modes', () => {
    const headers: AgentSessionHeader[] = Array.from({ length: 1000 }, (_, i) =>
      header({
        sessionId: `s${i}`,
        repoPath: `C:/code/repo-${i % 7}`,
        repoName: `repo-${i % 7}`,
        updatedAt: new Date(NOW - i * 3_600_000).toISOString(),
        changedFileCount: i % 3,
        state: (['ready', 'working', 'finished'] as const)[i % 3],
      })
    )
    for (const mode of ['recent', 'project', 'diff'] as const) {
      const rows = buildSidebarRows(headers, { mode, collapsedGroupIds: new Set(), now: NOW })
      const sessionIds = rows.filter((r) => r.kind === 'session').map((r) => r.id)
      expect(sessionIds).toHaveLength(1000)
      expect(new Set(sessionIds).size).toBe(1000)
    }
  })
})

describe('resolveSessionRepoFilter', () => {
  it('is app-wide (null) when the "This project only" toggle is off, regardless of the current repo', () => {
    expect(resolveSessionRepoFilter(false, 'repo-1')).toBeNull()
    expect(resolveSessionRepoFilter(false, null)).toBeNull()
  })

  it('scopes to the current repo when the toggle is on', () => {
    expect(resolveSessionRepoFilter(true, 'repo-1')).toBe('repo-1')
  })

  it('falls back to app-wide when the toggle is on but no repo has resolved yet -- never filters to "nothing"', () => {
    expect(resolveSessionRepoFilter(true, null)).toBeNull()
  })
})

describe('summarizeAgentActivity', () => {
  const h = (state: string) => ({ state })

  it('says nothing when nothing is happening', () => {
    expect(summarizeAgentActivity([h('finished'), h('draft')]).tone).toBeNull()
    expect(summarizeAgentActivity([]).tone).toBeNull()
  })

  it('reports work in progress', () => {
    const a = summarizeAgentActivity([h('working'), h('preparing'), h('finished')])
    expect(a.tone).toBe('working')
    expect(a.count).toBe(2)
    expect(a.label).toBe('2 chats are working')
  })

  it('puts a chat that needs the person ahead of one that is just busy', () => {
    // Waiting on a person is the state that cannot make progress without
    // them, so it outranks work that is still moving.
    const a = summarizeAgentActivity([h('working'), h('needsInput'), h('working')])
    expect(a.tone).toBe('needsYou')
    expect(a.count).toBe(1)
    expect(a.label).toBe('1 chat needs you')
  })

  it('uses the singular for one', () => {
    expect(summarizeAgentActivity([h('working')]).label).toBe('1 chat is working')
  })
})
