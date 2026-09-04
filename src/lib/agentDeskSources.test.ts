import { describe, expect, it } from 'vitest'
import {
  commitSourceInput,
  describeImportedAt,
  formatClock,
  describeSnapshotFreshness,
  diffSourceInput,
  explainRefreshSourceOutcome,
  issueSourceInput,
  openSpecChangeSourceInput,
  openSpecTaskSourceInput,
  pullRequestSourceInput,
  workingChangesSourceInput,
} from './agentDeskSources'
import type { IssueDetail, IssueSummary, PrDetail, PrSummary, SpecChange, SpecTask } from './bindings'

const issueSummary = (over: Partial<IssueSummary> = {}): IssueSummary => ({
  number: 42,
  title: 'Fix the drag preview',
  author: 'octocat',
  labels: [],
  assignee: null,
  comments: 0,
  updated_at: null,
  html_url: 'https://example.test/issues/42',
  ...over,
})

const issueDetail = (over: Partial<IssueDetail> = {}): IssueDetail => ({
  number: 42,
  title: 'Fix the drag preview',
  body: 'The preview lags behind the cursor.',
  author: 'octocat',
  state: 'open',
  labels: [],
  assignee: null,
  comments: [],
  html_url: 'https://example.test/issues/42',
  created_at: '2026-01-01T00:00:00Z',
  updated_at: null,
  ...over,
})

const prSummary = (over: Partial<PrSummary> = {}): PrSummary => ({
  number: 7,
  title: 'Add drag preview fix',
  author: 'octocat',
  author_is_bot: false,
  draft: false,
  head_ref: 'fix/drag-preview',
  base_ref: 'main',
  updated_at: null,
  created_at: '2026-01-01T00:00:00Z',
  html_url: 'https://example.test/pull/7',
  ...over,
})

const prDetail = (over: Partial<PrDetail> = {}): PrDetail => ({
  number: 7,
  title: 'Add drag preview fix',
  body: 'Fixes #42.',
  author: 'octocat',
  author_is_bot: false,
  state: 'open',
  draft: false,
  merged: false,
  mergeable: true,
  head_ref: 'fix/drag-preview',
  base_ref: 'main',
  additions: null,
  deletions: null,
  changed_files: null,
  comments: [],
  html_url: 'https://example.test/pull/7',
  created_at: '2026-01-01T00:00:00Z',
  updated_at: null,
  ...over,
})

describe('issueSourceInput', () => {
  it('carries number/title/url from a summary row with no body', () => {
    const input = issueSourceInput('github', 'acme', 'widgets', issueSummary())
    expect(input).toMatchObject({
      kind: 'issue',
      hostId: 'github',
      owner: 'acme',
      repo: 'widgets',
      number: 42,
      url: 'https://example.test/issues/42',
      title: 'Fix the drag preview',
    })
  })

  it('includes the body when a full detail is already loaded', () => {
    const input = issueSourceInput('github', 'acme', 'widgets', issueDetail())
    expect(input.summary).toContain('The preview lags behind the cursor.')
  })

  it('mentions labels and assignee when present', () => {
    const input = issueSourceInput(
      'github',
      'acme',
      'widgets',
      issueDetail({ labels: ['bug', 'ui'], assignee: 'hubot' })
    )
    expect(input.summary).toContain('bug')
    expect(input.summary).toContain('ui')
    expect(input.summary).toContain('hubot')
  })

  it('says unassigned when nobody is assigned', () => {
    const input = issueSourceInput('github', 'acme', 'widgets', issueDetail({ assignee: null }))
    expect(input.summary).toContain('unassigned')
  })

  it('never fetches anything -- pure function of its arguments', () => {
    // Regression guard for architecture.md section 8's "create from known
    // row data" rule: this function must be synchronous and side-effect
    // free so a caller can build a SessionSourceInput without awaiting
    // anything.
    const result = issueSourceInput('gitlab', 'acme', 'widgets', issueSummary())
    expect(result.hostId).toBe('gitlab')
  })
})

describe('pullRequestSourceInput', () => {
  it('carries head/base/number/url from a summary row', () => {
    const input = pullRequestSourceInput('github', 'acme', 'widgets', prSummary())
    expect(input).toMatchObject({
      kind: 'pullRequest',
      hostId: 'github',
      owner: 'acme',
      repo: 'widgets',
      number: 7,
      url: 'https://example.test/pull/7',
      head: 'fix/drag-preview',
      base: 'main',
      title: 'Add drag preview fix',
    })
  })

  it('describes draft state in the summary', () => {
    const input = pullRequestSourceInput('github', 'acme', 'widgets', prSummary({ draft: true }))
    expect(input.summary).toContain('draft')
  })

  it('describes merged state from a detail payload', () => {
    const input = pullRequestSourceInput('github', 'acme', 'widgets', prDetail({ merged: true }))
    expect(input.summary).toContain('merged')
  })

  it('describes open state when neither draft nor merged', () => {
    const input = pullRequestSourceInput('github', 'acme', 'widgets', prSummary({ draft: false }))
    expect(input.summary).toContain('open')
  })

  it('includes the body when a full detail is already loaded', () => {
    const input = pullRequestSourceInput('github', 'acme', 'widgets', prDetail())
    expect(input.summary).toContain('Fixes #42.')
  })
})

const specChange = (over: Partial<SpecChange> = {}): SpecChange => ({
  id: 'add-thing',
  title: 'Add thing',
  status: 'inBuild',
  progress: { done: 1, total: 3, percent: 33, is_draft: false },
  proposal: { why: 'Because users need it.', what_changes: '', impact: '', raw: '# Change: Add thing\n' },
  has_design: false,
  tasks: [],
  deltas: [],
  updated: 1700000000,
  notes: [],
  ...over,
})

const specTask = (over: Partial<SpecTask> = {}): SpecTask => ({
  index: 6,
  group: '2. Frontend',
  text: '2.4 Wire the kickoff button',
  done: false,
  line: 12,
  ...over,
})

describe('openSpecChangeSourceInput', () => {
  it('carries the change id and title', () => {
    const input = openSpecChangeSourceInput(specChange())
    expect(input).toMatchObject({ kind: 'openSpecChange', changeId: 'add-thing', title: 'Add thing' })
  })

  it('uses the Why section as the summary when present', () => {
    const input = openSpecChangeSourceInput(specChange())
    expect(input.summary).toBe('Because users need it.')
  })

  it('falls back to the raw proposal text when Why is empty', () => {
    const input = openSpecChangeSourceInput(
      specChange({ proposal: { why: '', what_changes: '', impact: '', raw: 'Unstructured notes.' } })
    )
    expect(input.summary).toBe('Unstructured notes.')
  })

  it('never fetches anything -- pure function of its arguments', () => {
    const input = openSpecChangeSourceInput(specChange({ id: 'other-change' }))
    expect(input.changeId).toBe('other-change')
  })
})

describe('openSpecTaskSourceInput', () => {
  it('carries the exact task index and text, not a recomputed next-open-task', () => {
    const change = specChange({
      tasks: [
        specTask({ index: 0, text: '1.1 First (still open)', done: false }),
        specTask({ index: 6, text: '2.4 Wire the kickoff button', done: false }),
      ],
    })
    const target = change.tasks[1]
    const input = openSpecTaskSourceInput(change, target)
    expect(input).toMatchObject({
      kind: 'openSpecTask',
      changeId: 'add-thing',
      taskIndex: 6,
      taskText: '2.4 Wire the kickoff button',
      title: 'Add thing',
    })
  })

  it('uses the task text as the summary', () => {
    const change = specChange()
    const task = specTask({ text: '3.1 Do the thing' })
    const input = openSpecTaskSourceInput(change, task)
    expect(input.summary).toBe('3.1 Do the thing')
  })
})

describe('describeSnapshotFreshness', () => {
  const now = Date.parse('2026-09-03T12:00:00Z')
  const at = (iso: string) => describeSnapshotFreshness(iso, false, now)

  it('says how long ago the source was checked, in words not a timestamp', () => {
    expect(at('2026-09-03T11:59:30Z')).toBe('Checked just now.')
    expect(at('2026-09-03T11:45:00Z')).toBe('Checked 15 minutes ago.')
    expect(at('2026-09-03T09:00:00Z')).toBe('Checked 3 hours ago.')
    expect(at('2026-08-29T12:00:00Z')).toBe('Checked 5 days ago.')
  })

  it('uses the singular where it should', () => {
    expect(at('2026-09-03T11:59:00Z')).toBe('Checked 1 minute ago.')
    expect(at('2026-09-03T11:00:00Z')).toBe('Checked 1 hour ago.')
    expect(at('2026-09-02T12:00:00Z')).toBe('Checked 1 day ago.')
  })

  it('stops counting rather than claiming precision it does not have', () => {
    expect(at('2024-01-01T00:00:00Z')).toBe('Checked a long time ago.')
  })

  it('says the panel is showing a saved copy when the live source is gone', () => {
    // "No longer available" alone tells the person the source is gone without
    // saying what they are still looking at.
    const line = describeSnapshotFreshness('2026-09-03T09:00:00Z', true, now)
    expect(line).toMatch(/Saved copy from 3 hours ago/)
    expect(line).toMatch(/what the chat still shows/)
  })

  it('says nothing rather than guessing at an unreadable timestamp', () => {
    expect(describeSnapshotFreshness('not a date', false, now)).toBeNull()
  })
})

describe('explainRefreshSourceOutcome', () => {
  const session = {} as never

  it('distinguishes a real refresh from a check that found nothing changed', () => {
    expect(explainRefreshSourceOutcome({ kind: 'refreshed', session, changed: true }).message).toMatch(/now shows the current/i)
    expect(explainRefreshSourceOutcome({ kind: 'refreshed', session, changed: false }).message).toMatch(/had not changed/i)
  })

  it('says the saved copy was kept when the original could not be reached', () => {
    // Never overwrite the snapshot with nothing -- the chat still has to read
    // correctly, so the person is told which version they are looking at.
    const out = explainRefreshSourceOutcome({ kind: 'liveUnavailable', session, detail: 'offline' })
    expect(out.ok).toBe(false)
    expect(out.message).toMatch(/saved copy was kept/i)
    expect(out.message).toMatch(/offline/)
  })

  it('carries the reason through for every failure rather than a bare "failed"', () => {
    expect(explainRefreshSourceOutcome({ kind: 'damaged', reason: 'bad json' }).message).toMatch(/bad json/)
    expect(explainRefreshSourceOutcome({ kind: 'unavailable', detail: 'locked' }).message).toMatch(/locked/)
    expect(explainRefreshSourceOutcome({ kind: 'writeFailed', detail: 'disk full' }).message).toMatch(/disk full/)
    expect(explainRefreshSourceOutcome({ kind: 'notFound' }).ok).toBe(false)
  })
})

describe('commit, diff and working-changes sources', () => {
  // These three variants were typed, persisted, converted by the backend and
  // rendered by the UI with no builder anywhere -- so nothing in the app could
  // create one, and the vision's "start a chat from a commit or a diff" named
  // a capability no gesture could reach.
  it('a commit carries its message, not just its id', () => {
    const src = commitSourceInput('abc1234def', 'Fix the login redirect', 'Ada', '2 days ago')
    expect(src.kind).toBe('commit')
    expect(src.oid).toBe('abc1234def')
    expect(src.title).toBe('Fix the login redirect')
    expect(src.summary).toMatch(/by Ada/)
    expect(src.summary).toMatch(/2 days ago/)
  })

  it('a commit with no subject still gets a readable title', () => {
    expect(commitSourceInput('abc1234def', '', '', '').title).toBe('Commit abc1234')
  })

  it('a diff keeps the scope verbatim so it still says what it pointed at', () => {
    const src = diffSourceInput('staged', ['a.ts', 'b.ts'], 'Staged changes')
    expect(src.scope).toBe('staged')
    expect(src.paths).toEqual(['a.ts', 'b.ts'])
    expect(src.summary).toBe('2 changed files')
  })

  it('one changed file is singular', () => {
    expect(diffSourceInput('unstaged', ['a.ts'], 'Your changes').summary).toBe('1 changed file')
  })

  it('working changes name the files, and stop naming them past a handful', () => {
    const many = ['a', 'b', 'c', 'd', 'e', 'f', 'g']
    const src = workingChangesSourceInput(many)
    expect(src.title).toBe('Your 7 changed files')
    expect(src.summary).toMatch(/and 2 more$/)
  })

  it('working changes say plainly when there are none', () => {
    expect(workingChangesSourceInput([]).summary).toBe('Nothing is changed right now')
  })
})

describe('describeImportedAt', () => {
  const now = Date.parse('2026-09-04T12:00:00Z')

  it('says when the message arrived here, not when it was written', () => {
    expect(describeImportedAt('2026-09-04T11:00:00Z', now)).toBe('Brought into GitWyrm 1 hour ago')
  })
  it('uses the same words as the snapshot freshness line', () => {
    expect(describeImportedAt('2026-09-04T11:59:30Z', now)).toBe('Brought into GitWyrm just now')
    expect(describeImportedAt('2026-08-30T12:00:00Z', now)).toBe('Brought into GitWyrm 5 days ago')
  })
  it('claims no time it does not know', () => {
    expect(describeImportedAt('not a date', now)).toBeNull()
  })
})

describe('formatClock', () => {
  it('shows a wall-clock time', () => {
    // Locale-dependent, so assert the shape rather than an exact string.
    expect(formatClock('2026-09-04T14:05:00Z')).toMatch(/\d{1,2}[:.]\d{2}/)
  })
  it('says nothing rather than "Invalid Date"', () => {
    expect(formatClock('not a date')).toBe('')
    expect(formatClock('')).toBe('')
  })
})
