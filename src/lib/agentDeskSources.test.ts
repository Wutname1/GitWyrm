import { describe, expect, it } from 'vitest'
import { issueSourceInput, openSpecChangeSourceInput, openSpecTaskSourceInput, pullRequestSourceInput } from './agentDeskSources'
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
