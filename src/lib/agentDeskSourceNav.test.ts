import { describe, expect, it } from 'vitest'
import type { SessionSource, SourceSnapshot } from '@/lib/bindings'
import { resolveSourceNav } from './agentDeskSourceNav'

const snapshot: SourceSnapshot = {
  title: 'A title',
  summary: 'A summary',
  capturedAt: '2026-01-01T00:00:00Z',
  liveUnavailable: false,
}

describe('resolveSourceNav', () => {
  it('marks manual sources unreachable with an honest reason', () => {
    const source: SessionSource = { kind: 'manual', repoId: 'repo-1' }
    const action = resolveSourceNav(source)
    expect(action.kind).toBe('unreachable')
    if (action.kind !== 'unreachable') throw new Error('expected unreachable')
    expect(action.reason.length).toBeGreaterThan(0)
  })

  it('routes an issue source to the github panel by number', () => {
    const source: SessionSource = {
      kind: 'issue',
      hostId: 'github.com',
      owner: 'acme',
      repo: 'widget',
      number: 42,
      url: 'https://github.com/acme/widget/issues/42',
      snapshot,
    }
    const action = resolveSourceNav(source)
    expect(action).toEqual({ kind: 'github', itemKind: 'issue', number: 42 })
  })

  it('routes a pull request source to the github panel as a pr', () => {
    const source: SessionSource = {
      kind: 'pullRequest',
      hostId: 'github.com',
      owner: 'acme',
      repo: 'widget',
      number: 7,
      url: 'https://github.com/acme/widget/pull/7',
      head: 'feature',
      base: 'main',
      snapshot,
    }
    const action = resolveSourceNav(source)
    expect(action).toEqual({ kind: 'github', itemKind: 'pr', number: 7 })
  })

  it('routes an openSpecChange source to the change selection', () => {
    const source: SessionSource = { kind: 'openSpecChange', changeId: 'add-thing', snapshot }
    const action = resolveSourceNav(source)
    expect(action).toEqual({ kind: 'openspecChange', changeId: 'add-thing' })
  })

  it('routes an openSpecTask source to its parent change, honestly (no per-task surface exists)', () => {
    const source: SessionSource = {
      kind: 'openSpecTask',
      changeId: 'add-thing',
      taskIndex: 3,
      taskText: 'Do the thing',
      snapshot,
    }
    const action = resolveSourceNav(source)
    expect(action).toEqual({ kind: 'openspecChange', changeId: 'add-thing' })
  })

  it('routes a commit source to the commit oid', () => {
    const source: SessionSource = { kind: 'commit', oid: 'abc123', snapshot }
    const action = resolveSourceNav(source)
    expect(action).toEqual({ kind: 'commit', oid: 'abc123' })
  })

  it('routes a diff source to its first path', () => {
    const source: SessionSource = { kind: 'diff', scope: 'src/**', paths: ['src/a.ts', 'src/b.ts'], snapshot }
    const action = resolveSourceNav(source)
    expect(action).toEqual({ kind: 'diff', path: 'src/a.ts' })
  })

  it('routes a diff source with no paths to a null path rather than crashing', () => {
    const source: SessionSource = { kind: 'diff', scope: 'src/**', paths: [], snapshot }
    const action = resolveSourceNav(source)
    expect(action).toEqual({ kind: 'diff', path: null })
  })

  it('routes a workingChanges source to its first path', () => {
    const source: SessionSource = { kind: 'workingChanges', paths: ['README.md'], snapshot }
    const action = resolveSourceNav(source)
    expect(action).toEqual({ kind: 'diff', path: 'README.md' })
  })

  it('marks checkFailure sources unreachable with an honest reason', () => {
    const source: SessionSource = {
      kind: 'checkFailure',
      provider: 'github-actions',
      checkId: 'ci/build',
      url: null,
      snapshot,
    }
    const action = resolveSourceNav(source)
    expect(action.kind).toBe('unreachable')
  })
})
