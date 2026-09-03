import { describe, expect, it } from 'vitest'
import type { MessageTarget } from '@/lib/bindings'
import { diffScopePath, resolveMessageTarget, type MessageTargetContext } from './agentDeskTargets'

const lead = { executionId: 'lead-1', parentExecutionId: null, worktreePath: null }
const builder = { executionId: 'helper-1', parentExecutionId: 'lead-1', worktreePath: 'C:/wt/helper-1' }
const reader = { executionId: 'helper-2', parentExecutionId: 'lead-1', worktreePath: null }

/** A chat with a project, a lead and two helpers (one with a worktree, one read-only). */
const teamChat: MessageTargetContext = {
  repoPath: 'C:/repo',
  executions: [lead, builder, reader],
  messageExecutionId: null,
  hasGraph: true,
}

/** A chat with one agent working alone. */
const soloChat: MessageTargetContext = {
  repoPath: 'C:/repo',
  executions: [lead],
  messageExecutionId: 'lead-1',
  hasGraph: false,
}

/** A chat that was never linked to a project. */
const noProjectChat: MessageTargetContext = {
  repoPath: null,
  executions: [],
  messageExecutionId: null,
  hasGraph: false,
}

describe('resolveMessageTarget: source', () => {
  it('resolves source targets to the shared open-source path', () => {
    const resolved = resolveMessageTarget({ kind: 'source' }, teamChat)
    expect(resolved.kind).toBe('source')
  })
})

describe('resolveMessageTarget: file', () => {
  it('opens a file from a helper with a worktree in that worktree', () => {
    const resolved = resolveMessageTarget(
      { kind: 'file', path: 'src/lib/foo.ts' },
      { ...teamChat, messageExecutionId: 'helper-1' }
    )
    expect(resolved).toEqual({
      kind: 'diff',
      label: 'src/lib/foo.ts',
      path: 'src/lib/foo.ts',
      worktreePath: 'C:/wt/helper-1',
      location: 'helperWorktree',
    })
  })

  it('falls back to the chat repository for a read-only helper', () => {
    const resolved = resolveMessageTarget(
      { kind: 'file', path: 'src/lib/foo.ts' },
      { ...teamChat, messageExecutionId: 'helper-2' }
    )
    expect(resolved).toMatchObject({ kind: 'diff', worktreePath: 'C:/repo', location: 'repo' })
  })

  it('falls back to the chat repository for a message with no execution', () => {
    const resolved = resolveMessageTarget({ kind: 'file', path: 'README.md' }, teamChat)
    expect(resolved).toMatchObject({ kind: 'diff', path: 'README.md', worktreePath: 'C:/repo', location: 'repo' })
  })

  it('is unavailable, with the path as the label, when the chat has no project', () => {
    const resolved = resolveMessageTarget({ kind: 'file', path: 'src/lib/foo.ts' }, noProjectChat)
    expect(resolved.kind).toBe('unavailable')
    expect(resolved.label).toBe('src/lib/foo.ts')
    if (resolved.kind !== 'unavailable') throw new Error('expected unavailable')
    expect(resolved.reason).toMatch(/project/)
  })
})

describe('resolveMessageTarget: diff', () => {
  it('opens a concrete scope as that file in the sending helper worktree', () => {
    const resolved = resolveMessageTarget(
      { kind: 'diff', scope: 'src/lib/foo.ts' },
      { ...teamChat, messageExecutionId: 'helper-1' }
    )
    expect(resolved).toEqual({
      kind: 'diff',
      label: 'src/lib/foo.ts',
      path: 'src/lib/foo.ts',
      worktreePath: 'C:/wt/helper-1',
      location: 'helperWorktree',
    })
  })

  it('opens a glob scope as the repository with no file selected, keeping the scope as the label', () => {
    const resolved = resolveMessageTarget({ kind: 'diff', scope: 'src/**' }, teamChat)
    expect(resolved).toMatchObject({ kind: 'diff', label: 'src/**', path: null, worktreePath: 'C:/repo' })
  })

  it('uses a generic label and no file when the scope is empty', () => {
    const resolved = resolveMessageTarget({ kind: 'diff', scope: '' }, teamChat)
    expect(resolved).toMatchObject({ kind: 'diff', label: 'View diff', path: null })
  })

  it('is unavailable when the chat has no project', () => {
    const resolved = resolveMessageTarget({ kind: 'diff', scope: 'src/**' }, noProjectChat)
    expect(resolved.kind).toBe('unavailable')
    expect(resolved.label).toBe('src/**')
  })
})

describe('diffScopePath', () => {
  it('keeps a plain file path and trims it', () => {
    expect(diffScopePath('  src/a.ts ')).toBe('src/a.ts')
  })

  it('rejects globs and empty scopes', () => {
    expect(diffScopePath('src/**')).toBeNull()
    expect(diffScopePath('src/{a,b}.ts')).toBeNull()
    expect(diffScopePath('src/?.ts')).toBeNull()
    expect(diffScopePath('')).toBeNull()
    expect(diffScopePath('   ')).toBeNull()
  })
})

describe('resolveMessageTarget: graphNode', () => {
  it('resolves to the graph panel for an agent the chat still lists', () => {
    const resolved = resolveMessageTarget({ kind: 'graphNode', executionId: 'helper-1' }, teamChat)
    expect(resolved).toEqual({ kind: 'graphNode', label: 'View in graph', executionId: 'helper-1' })
  })

  it('is unavailable for an agent the chat no longer lists', () => {
    const resolved = resolveMessageTarget({ kind: 'graphNode', executionId: 'gone' }, teamChat)
    expect(resolved.kind).toBe('unavailable')
    if (resolved.kind !== 'unavailable') throw new Error('expected unavailable')
    expect(resolved.reason).toMatch(/no longer/)
  })

  it('is unavailable in a chat with one agent working alone', () => {
    const resolved = resolveMessageTarget({ kind: 'graphNode', executionId: 'lead-1' }, soloChat)
    expect(resolved.kind).toBe('unavailable')
    if (resolved.kind !== 'unavailable') throw new Error('expected unavailable')
    expect(resolved.reason).toMatch(/alone/)
  })
})

describe('resolveMessageTarget: openSpecTask', () => {
  it('resolves to the change with a 1-based task label', () => {
    const target: MessageTarget = { kind: 'openSpecTask', changeId: 'change-1', taskIndex: 2 }
    const resolved = resolveMessageTarget(target, teamChat)
    expect(resolved).toEqual({ kind: 'openSpecTask', label: 'Task 3', changeId: 'change-1', taskIndex: 2 })
  })

  it('does not need a project link to resolve; the click checks that later', () => {
    const resolved = resolveMessageTarget({ kind: 'openSpecTask', changeId: 'c', taskIndex: 0 }, noProjectChat)
    expect(resolved.kind).toBe('openSpecTask')
  })
})

describe('resolveMessageTarget: unavailable reasons', () => {
  it('never returns an empty reason for an unavailable target', () => {
    const cases: [MessageTarget, MessageTargetContext][] = [
      [{ kind: 'file', path: 'a.ts' }, noProjectChat],
      [{ kind: 'diff', scope: 'a' }, noProjectChat],
      [{ kind: 'graphNode', executionId: 'e' }, teamChat],
      [{ kind: 'graphNode', executionId: 'lead-1' }, soloChat],
    ]
    for (const [target, ctx] of cases) {
      const resolved = resolveMessageTarget(target, ctx)
      expect(resolved.kind).toBe('unavailable')
      if (resolved.kind === 'unavailable') {
        expect(resolved.reason.trim().length).toBeGreaterThan(0)
      }
    }
  })
})
