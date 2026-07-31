import { describe, it, expect } from 'vitest'
import { diffLines, countChanges, isUnchanged } from './textDiff'

/**
 * Strip a string to the lines `diffLines` works on, for the round-trip check.
 * Mirrors `toLines`: CRLF normalized, one trailing newline dropped.
 */
function lines(text: string): string[] {
  const out = text.replace(/\r\n/g, '\n').split('\n')
  if (out.length > 1 && out[out.length - 1] === '') out.pop()
  return out
}

/**
 * The property the rendered diff depends on: reading the output while ignoring
 * additions reproduces the "before" exactly, and ignoring removals reproduces
 * the "after" exactly. If this holds, the view cannot silently drop or invent a
 * line -- which is the failure that would make a drafted edit untrustworthy.
 */
function preservesBothSides(before: string, after: string): boolean {
  const d = diffLines(before, after)
  const left = d.filter((l) => l.sign !== '+').map((l) => l.text)
  const right = d.filter((l) => l.sign !== '-').map((l) => l.text)
  return (
    JSON.stringify(left) === JSON.stringify(lines(before)) &&
    JSON.stringify(right) === JSON.stringify(lines(after))
  )
}

const PROPOSAL = '# Change\n\n## Why\n\nBecause it is broken.\n'

describe('diffLines', () => {
  it('reports no change for identical text', () => {
    const d = diffLines(PROPOSAL, PROPOSAL)
    expect(isUnchanged(d)).toBe(true)
    expect(countChanges(d)).toEqual({ added: 0, removed: 0 })
  })

  it('marks a reworded line as one removal and one addition', () => {
    const after = '# Change\n\n## Why\n\nBecause it is thoroughly broken.\n'
    expect(countChanges(diffLines(PROPOSAL, after))).toEqual({ added: 1, removed: 1 })
  })

  it('emits the removal before its replacement', () => {
    // computeWordSpans pairs a run of removals with the run of additions that
    // follows it. The other order would silently disable word highlighting.
    const d = diffLines(PROPOSAL, '# Change\n\n## Why\n\nA different reason.\n')
    const removed = d.findIndex((l) => l.sign === '-')
    const added = d.findIndex((l) => l.sign === '+')
    expect(removed).toBeGreaterThanOrEqual(0)
    expect(added).toBeGreaterThan(removed)
  })

  it('does not invent a change when only the trailing newline differs', () => {
    // The editor round-trips the newline; it must not read as an edit.
    expect(isUnchanged(diffLines('a\nb\n', 'a\nb'))).toBe(true)
  })

  it('treats CRLF and LF as the same text', () => {
    expect(isUnchanged(diffLines('a\r\nb\r\n', 'a\nb\n'))).toBe(true)
  })

  it('handles a pure insertion', () => {
    const d = diffLines('a\nb\nc\n', 'a\nNEW\nb\nc\n')
    expect(countChanges(d)).toEqual({ added: 1, removed: 0 })
    expect(preservesBothSides('a\nb\nc\n', 'a\nNEW\nb\nc\n')).toBe(true)
  })

  it('handles a pure deletion', () => {
    expect(countChanges(diffLines('a\nb\nc\n', 'a\nc\n'))).toEqual({ added: 0, removed: 1 })
  })

  it('handles writing a file that did not exist', () => {
    // The first save of design.md diffs against an empty string.
    expect(preservesBothSides('', '# Design\n\nStuff.\n')).toBe(true)
    expect(preservesBothSides('# Design\n', '')).toBe(true)
  })

  it('keeps unchanged context around a realistic reword', () => {
    const before =
      '# Change: thing\n\n## Why\n\nThe old reason, which was fine.\nA second line here.\n\n## What Changes\n\n- one\n- two\n'
    const after =
      '# Change: thing\n\n## Why\n\nThe new reason, much better.\nA second line here.\n\n## What Changes\n\n- one\n- two\n- three\n'
    const d = diffLines(before, after)
    expect(countChanges(d)).toEqual({ added: 2, removed: 1 })
    // Most of the file is untouched and must render as context, not as a
    // wholesale replacement.
    expect(d.filter((l) => l.sign === ' ').length).toBeGreaterThan(5)
    expect(preservesBothSides(before, after)).toBe(true)
  })

  it('preserves both sides for every shape it is given', () => {
    const cases: Array<[string, string]> = [
      [PROPOSAL, '# Change\n\n## Why\n\nBecause it is thoroughly broken.\n'],
      ['a\nb\nc\n', 'c\nb\na\n'],
      ['same\n', 'same\n'],
      ['one\n', 'one\ntwo\nthree\n'],
      ['one\ntwo\nthree\n', 'one\n'],
    ]
    for (const [before, after] of cases) {
      expect(preservesBothSides(before, after)).toBe(true)
    }
  })

  it('degrades instead of stalling on a very large replacement', () => {
    // Past the cell cap the middle is emitted as a wholesale replacement. The
    // guarantee that weakens is alignment quality, never correctness: both
    // sides must still be reproducible from the output.
    const a = Array.from({ length: 2600 }, (_, i) => `x${i}`).join('\n')
    const b = Array.from({ length: 2600 }, (_, i) => `y${i}`).join('\n')
    const started = Date.now()
    const d = diffLines(a, b)
    expect(Date.now() - started).toBeLessThan(2000)
    expect(countChanges(d)).toEqual({ added: 2600, removed: 2600 })
    expect(preservesBothSides(a, b)).toBe(true)
  })
})
