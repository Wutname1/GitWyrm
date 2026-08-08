import { describe, expect, it } from 'vitest'
import { buildRows, discardedCount, type PreviewMode } from './syncPreview'

const ours = [
  { sha: 'aaa1111', summary: 'my only change' },
]
const theirs = [
  { sha: 'bbb4444', summary: 'theirs 4' },
  { sha: 'bbb3333', summary: 'theirs 3' },
  { sha: 'bbb2222', summary: 'theirs 2' },
  { sha: 'bbb1111', summary: 'theirs 1' },
]

/** The ahead-1/behind-4 shape the preview was built for. */
const build = (mode: PreviewMode | null) => buildRows(ours, theirs, mode, 'main', 'origin/main')

describe('buildRows', () => {
  it('marks THEIR commits as lost when force-pushing', () => {
    const rows = build('replace')
    expect(discardedCount(rows)).toBe(4)
    expect(rows.filter((r) => r.side === 'theirs').every((r) => r.fate === 'discard')).toBe(true)
    // Our own work survives a force-push -- it is what replaces theirs.
    expect(rows.filter((r) => r.side === 'ours').every((r) => r.fate === 'keep')).toBe(true)
  })

  it('marks OUR commits as lost when resetting, the mirror of force-push', () => {
    const rows = build('reset')
    expect(discardedCount(rows)).toBe(1)
    expect(rows.find((r) => r.side === 'ours')?.fate).toBe('discard')
    expect(rows.filter((r) => r.side === 'theirs').every((r) => r.fate === 'keep')).toBe(true)
  })

  it('rebuilds our commits on top when stacking, losing nothing', () => {
    const rows = build('stack')
    expect(discardedCount(rows)).toBe(0)
    expect(rows[0].fate).toBe('replay')
    expect(rows[0].side).toBe('ours')
    // Their commits are untouched and sit below ours.
    expect(rows.filter((r) => r.side === 'theirs').every((r) => r.fate === 'keep')).toBe(true)
  })

  it('adds a merge row and keeps both arms when blending', () => {
    const rows = build('blend')
    expect(discardedCount(rows)).toBe(0)
    expect(rows[0].fate).toBe('merge')
    expect(rows[0].sha).toBeNull()
    expect(rows[0].summary).toBe('Blend origin/main into main')
    expect(rows).toHaveLength(1 + ours.length + theirs.length)
  })

  it('shows only the incoming commits for a fast-forward', () => {
    const rows = build('catch-up')
    expect(discardedCount(rows)).toBe(0)
    expect(rows).toHaveLength(theirs.length)
    expect(rows.every((r) => r.side === 'theirs')).toBe(true)
  })

  it('shows both arms untouched with no mode selected', () => {
    const rows = build(null)
    expect(discardedCount(rows)).toBe(0)
    expect(rows).toHaveLength(ours.length + theirs.length)
    expect(rows.every((r) => r.fate === 'keep')).toBe(true)
  })

  it('gives every row a distinct key so the two arms never collide', () => {
    const keys = build('blend').map((r) => r.key)
    expect(new Set(keys).size).toBe(keys.length)
  })

  it('reports nothing lost when one side has no commits', () => {
    // A clean fast-forward: nothing of ours is at risk under any option.
    const rows = buildRows([], theirs, 'replace', 'main', 'origin/main')
    expect(discardedCount(rows)).toBe(4)
    const resetRows = buildRows([], theirs, 'reset', 'main', 'origin/main')
    expect(discardedCount(resetRows)).toBe(0)
  })
})
