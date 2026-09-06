import { describe, expect, it } from 'vitest'
import type { SessionSource, SourceSnapshot } from '@/lib/bindings'
import { describeSource } from './NewChatLanding'

/**
 * `describeSource` writes the "What started this?" line on the first screen a
 * new person meets, and nothing was checking it.
 *
 * The line it produced for a one-file change read "1 file(s) changed" --
 * developer shorthand, on the one screen where a beginner is most likely to
 * be reading carefully. Every other place that counts files spells it out,
 * including `SessionContextPanel`, which describes these same two sources.
 */

const snapshot = (title: string): SourceSnapshot => ({
  title,
  summary: '',
  capturedAt: '2026-01-01T00:00:00Z',
  liveUnavailable: false,
})

describe('describeSource', () => {
  it('says nothing for a chat nothing started', () => {
    expect(describeSource(null)).toBeNull()
    expect(describeSource({ kind: 'manual', repoId: 'repo-1' })).toBeNull()
  })

  it('counts one file as a file, not "file(s)"', () => {
    const one: SessionSource = {
      kind: 'diff',
      scope: 'head',
      paths: ['src/a.rs'],
      snapshot: snapshot(''),
    }
    expect(describeSource(one)?.detail).toBe('1 file changed')
  })

  it('counts several files as files', () => {
    const many: SessionSource = {
      kind: 'workingChanges',
      paths: ['src/a.rs', 'src/b.rs', 'src/c.rs'],
      snapshot: snapshot(''),
    }
    expect(describeSource(many)?.detail).toBe('3 files not yet committed')
  })

  /** Rule #2: this is the first screen, read by someone who does not code. */
  it('never shows the "(s)" shorthand for any source', () => {
    const sources: SessionSource[] = [
      { kind: 'diff', scope: 'head', paths: ['a'], snapshot: snapshot('') },
      { kind: 'workingChanges', paths: ['a'], snapshot: snapshot('') },
      {
        kind: 'issue',
        hostId: 'github',
        owner: 'o',
        repo: 'r',
        number: 7,
        url: 'https://example.test/7',
        snapshot: snapshot(''),
      },
      { kind: 'commit', oid: 'abcdef1234567890', snapshot: snapshot('') },
    ]
    for (const source of sources) {
      const described = describeSource(source)
      expect(described?.title ?? '').not.toContain('(s)')
      expect(described?.detail ?? '').not.toContain('(s)')
    }
  })

  /**
   * The snapshot title is what the person recognises. It is captured by the
   * backend when the chat starts, so it is shown as-is; only its absence
   * falls back to naming the kind.
   */
  it('leads with the snapshot title, and names the kind when there is none', () => {
    const titled: SessionSource = {
      kind: 'issue',
      hostId: 'github',
      owner: 'acme',
      repo: 'widgets',
      number: 318,
      url: 'https://example.test/318',
      snapshot: snapshot('Crash when the list is empty'),
    }
    expect(describeSource(titled)?.title).toBe('Crash when the list is empty')
    expect(describeSource(titled)?.detail).toBe('Issue #318 in acme/widgets')

    const untitled: SessionSource = { ...titled, snapshot: snapshot('') }
    expect(describeSource(untitled)?.title).toBe('Issue #318')
  })

  /** A commit is recognised by a short id, not the whole forty characters. */
  it('shortens a commit id to something readable', () => {
    const commit: SessionSource = {
      kind: 'commit',
      oid: 'abcdef1234567890abcdef1234567890abcdef12',
      snapshot: snapshot(''),
    }
    expect(describeSource(commit)?.detail).toBe('Commit abcdef12')
  })

  /**
   * An imported conversation must stay attributed to the tool it came from
   * -- the product treats importing, continuing here, and resuming in the
   * original tool as three different things that must never look alike.
   */
  it('names the tool an imported conversation came from', () => {
    const imported: SessionSource = {
      kind: 'imported',
      adapterId: 'claude-code',
      externalSessionId: 'ext-1',
      snapshot: snapshot('Refactor the parser'),
    }
    const described = describeSource(imported)
    expect(described?.title).toBe('Refactor the parser')
    expect(described?.detail).toMatch(/^From /)
  })

  /** Every kind must produce something; a blank row explains nothing. */
  it('gives every source that started a chat a title', () => {
    const sources: SessionSource[] = [
      { kind: 'openSpecChange', changeId: 'add-the-thing', snapshot: snapshot('') },
      {
        kind: 'openSpecTask',
        changeId: 'add-the-thing',
        taskIndex: 3,
        taskText: 'Write the parser',
        snapshot: snapshot(''),
      },
      {
        kind: 'checkFailure',
        provider: 'GitHub Actions',
        checkId: 'build',
        url: null,
        snapshot: snapshot(''),
      },
    ]
    for (const source of sources) {
      const described = describeSource(source)
      expect(described?.title.trim()).toBeTruthy()
    }
  })
})
