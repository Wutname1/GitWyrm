import { describe, it, expect } from 'vitest'
import {
  conflictsOf,
  isDecided,
  detectEol,
  hasMarkers,
  parseConflict,
  renderSections,
  resolvedCount,
  type ConflictSection,
  type Section,
} from './conflictHunks'

/** The conflict sections of `text`, for assertions about contested lines. */
const conflicts = (text: string): ConflictSection[] => conflictsOf(parseConflict(text))

/** Join lines the way a fixture reads, so tests stay legible. */
const lf = (...lines: string[]) => lines.join('\n') + '\n'
const crlf = (...lines: string[]) => lines.join('\r\n') + '\r\n'

/**
 * Real `git merge` output, captured from a scratch repository rather than
 * written by hand -- the label text and the exact marker spellings are the
 * things most easily got wrong from memory.
 */
const ZDIFF3 = lf(
  'line1',
  '<<<<<<< HEAD',
  'OURS-A',
  '||||||| 566a06a',
  'line2',
  '=======',
  'THEIRS-A',
  '>>>>>>> other',
  'line3'
)

/** The same conflict as git's default (non-diff3) style writes it: no base. */
const MERGE_STYLE = lf(
  'line1',
  '<<<<<<< HEAD',
  'OURS-A',
  '=======',
  'THEIRS-A',
  '>>>>>>> other',
  'line3'
)

describe('detectEol', () => {
  it('reads a CRLF file as CRLF', () => {
    expect(detectEol('a\r\nb\r\n')).toBe('\r\n')
  })

  it('reads an LF file as LF', () => {
    expect(detectEol('a\nb\n')).toBe('\n')
  })

  /**
   * Mixed endings follow the majority. Rewriting every line to match a stray
   * first line would show the whole file as modified in the next diff.
   */
  it('follows the majority in a mixed file', () => {
    expect(detectEol('a\r\nb\nc\nd\n')).toBe('\n')
    expect(detectEol('a\nb\r\nc\r\nd\r\n')).toBe('\r\n')
  })

  it('treats a file with no newline at all as LF', () => {
    expect(detectEol('single line')).toBe('\n')
  })
})

describe('hasMarkers', () => {
  it('finds markers in conflicted text', () => {
    expect(hasMarkers(ZDIFF3)).toBe(true)
  })

  it('reports none for a clean file', () => {
    expect(hasMarkers(lf('line1', 'line2'))).toBe(false)
  })

  /**
   * The case the marker guard exists for: the user deleted the opening and
   * closing markers by hand but left the separator behind. Committing this
   * writes `=======` into the source file.
   */
  it('finds a lone separator left behind by hand editing', () => {
    expect(hasMarkers(lf('line1', '=======', 'line2'))).toBe(true)
  })

  /**
   * CRLF puts the \r inside the marker line, so an equality test against
   * '=======' is false for every marker in the file.
   */
  it('finds markers in a CRLF file', () => {
    expect(hasMarkers(crlf('a', '<<<<<<< HEAD', 'b', '=======', 'c', '>>>>>>> x'))).toBe(true)
  })

  /** A row of equals signs in a document is not a conflict marker. */
  it('does not mistake a longer run of the same character for a marker', () => {
    expect(hasMarkers(lf('title', '========', 'body'))).toBe(false)
  })
})

describe('parseConflict', () => {
  it('splits stable text from the conflict', () => {
    const sections = parseConflict(ZDIFF3)
    expect(sections.map((s) => s.kind)).toEqual(['stable', 'conflict', 'stable'])
    expect((sections[0] as Extract<Section, { kind: 'stable' }>).lines).toEqual(['line1'])
    expect((sections[2] as Extract<Section, { kind: 'stable' }>).lines).toEqual(['line3'])
  })

  it('reads all three sides and both labels from diff3 markers', () => {
    const [c] = conflicts(ZDIFF3)
    expect(c.ours).toEqual(['OURS-A'])
    expect(c.base).toEqual(['line2'])
    expect(c.theirs).toEqual(['THEIRS-A'])
    expect(c.oursLabel).toBe('HEAD')
    expect(c.theirsLabel).toBe('other')
  })

  /** Without diff3 there is no ancestor section; the other two still parse. */
  it('handles markers with no base section', () => {
    const [c] = conflicts(MERGE_STYLE)
    expect(c.ours).toEqual(['OURS-A'])
    expect(c.base).toEqual([])
    expect(c.theirs).toEqual(['THEIRS-A'])
  })

  it('parses a CRLF conflict without leaving carriage returns on the lines', () => {
    const [c] = conflicts(
      crlf('line1', '<<<<<<< HEAD', 'OURS', '=======', 'THEIRS', '>>>>>>> other', 'line3')
    )
    expect(c.ours).toEqual(['OURS'])
    expect(c.theirs).toEqual(['THEIRS'])
  })

  /**
   * The payoff of asking git for diff3 markers: two independent edits stay two
   * separately-resolvable conflicts instead of one block that also swallows the
   * untouched lines between them.
   */
  it('keeps two independent conflicts separate', () => {
    const text = lf(
      'line1',
      '<<<<<<< HEAD',
      'OURS-A',
      '=======',
      'THEIRS-A',
      '>>>>>>> other',
      'line3',
      'line4',
      '<<<<<<< HEAD',
      'OURS-B',
      '=======',
      'THEIRS-B',
      '>>>>>>> other'
    )
    const found = conflicts(text)
    expect(found).toHaveLength(2)
    expect(found[0].ours).toEqual(['OURS-A'])
    expect(found[1].ours).toEqual(['OURS-B'])
    // Ids are positional so a choice keyed by id survives edits to stable text.
    expect(found.map((c) => c.id)).toEqual([0, 1])
  })

  it('treats a file with no markers as one stable section', () => {
    const sections = parseConflict(lf('a', 'b'))
    expect(sections).toHaveLength(1)
    expect(sections[0].kind).toBe('stable')
  })

  it('returns a section for an empty file', () => {
    expect(parseConflict('')).toHaveLength(1)
  })

  it('keeps an empty side empty rather than dropping the conflict', () => {
    const [c] = conflicts(lf('a', '<<<<<<< HEAD', '=======', 'THEIRS', '>>>>>>> other'))
    expect(c.ours).toEqual([])
    expect(c.theirs).toEqual(['THEIRS'])
  })

  describe('malformed markers', () => {
    /**
     * Half-edited files are the common case here, and the view must still be
     * able to show them. Every one of these parses to text rather than throwing.
     */
    it('keeps an unterminated conflict as ordinary text', () => {
      const text = lf('a', '<<<<<<< HEAD', 'OURS', 'no closing marker')
      expect(() => parseConflict(text)).not.toThrow()
      expect(conflicts(text)).toHaveLength(0)
      expect(renderSections(parseConflict(text), {})).toBe(text)
    })

    it('keeps an opener with no separator as ordinary text', () => {
      const text = lf('a', '<<<<<<< HEAD', 'OURS', '>>>>>>> other')
      expect(conflicts(text)).toHaveLength(0)
    })

    /**
     * A stray opener before a real conflict must not swallow it: the first is
     * re-read as text and the second still resolves normally.
     */
    it('still finds a later well-formed conflict after a stray opener', () => {
      const text = lf(
        '<<<<<<< stray',
        'a',
        '<<<<<<< HEAD',
        'OURS',
        '=======',
        'THEIRS',
        '>>>>>>> other'
      )
      const found = conflicts(text)
      expect(found).toHaveLength(1)
      expect(found[0].ours).toEqual(['OURS'])
    })
  })
})

describe('renderSections', () => {
  it('writes our side and drops the markers', () => {
    const sections = parseConflict(ZDIFF3)
    expect(renderSections(sections, { 0: 'ours' })).toBe(lf('line1', 'OURS-A', 'line3'))
  })

  it('writes their side', () => {
    const sections = parseConflict(ZDIFF3)
    expect(renderSections(sections, { 0: 'theirs' })).toBe(lf('line1', 'THEIRS-A', 'line3'))
  })

  it('writes the common ancestor when asked', () => {
    const sections = parseConflict(ZDIFF3)
    expect(renderSections(sections, { 0: 'base' })).toBe(lf('line1', 'line2', 'line3'))
  })

  it('keeps both sides, in the order asked for', () => {
    const sections = parseConflict(ZDIFF3)
    expect(renderSections(sections, { 0: 'both-ours-first' })).toBe(
      lf('line1', 'OURS-A', 'THEIRS-A', 'line3')
    )
    expect(renderSections(sections, { 0: 'both-theirs-first' })).toBe(
      lf('line1', 'THEIRS-A', 'OURS-A', 'line3')
    )
  })

  /**
   * An undecided conflict keeps its markers, so a partly-resolved file written
   * to disk is still a valid conflicted file rather than a silent pick of one
   * side. Round-tripping it must not drift.
   */
  it('round-trips an undecided conflict unchanged', () => {
    expect(renderSections(parseConflict(ZDIFF3), {})).toBe(ZDIFF3)
    expect(renderSections(parseConflict(MERGE_STYLE), {})).toBe(MERGE_STYLE)
  })

  it('rebuilds a CRLF file with CRLF endings', () => {
    const text = crlf('line1', '<<<<<<< HEAD', 'OURS', '=======', 'THEIRS', '>>>>>>> other')
    const sections = parseConflict(text)
    expect(renderSections(sections, { 0: 'ours' }, detectEol(text))).toBe(crlf('line1', 'OURS'))
  })

  it('preserves a missing trailing newline', () => {
    const text = 'line1\n<<<<<<< HEAD\nOURS\n=======\nTHEIRS\n>>>>>>> other'
    expect(renderSections(parseConflict(text), { 0: 'ours' })).toBe('line1\nOURS')
  })

  it('preserves a present trailing newline', () => {
    expect(renderSections(parseConflict(ZDIFF3), { 0: 'ours' }).endsWith('\n')).toBe(true)
  })

  /** Choosing every side reproduces that side's whole file. */
  it('reproduces our whole file when every conflict picks ours', () => {
    const text = lf(
      '<<<<<<< HEAD',
      'OURS-A',
      '=======',
      'THEIRS-A',
      '>>>>>>> other',
      'shared',
      '<<<<<<< HEAD',
      'OURS-B',
      '=======',
      'THEIRS-B',
      '>>>>>>> other'
    )
    const sections = parseConflict(text)
    expect(renderSections(sections, { 0: 'ours', 1: 'ours' })).toBe(
      lf('OURS-A', 'shared', 'OURS-B')
    )
  })

  it('resolves each conflict independently', () => {
    const text = lf(
      '<<<<<<< HEAD',
      'OURS-A',
      '=======',
      'THEIRS-A',
      '>>>>>>> other',
      'shared',
      '<<<<<<< HEAD',
      'OURS-B',
      '=======',
      'THEIRS-B',
      '>>>>>>> other'
    )
    const sections = parseConflict(text)
    expect(renderSections(sections, { 0: 'ours', 1: 'theirs' })).toBe(
      lf('OURS-A', 'shared', 'THEIRS-B')
    )
  })

  /** The rendered result must never still be conflicted. */
  it('produces marker-free text once every conflict is decided', () => {
    const sections = parseConflict(ZDIFF3)
    expect(hasMarkers(renderSections(sections, { 0: 'ours' }))).toBe(false)
  })
})

describe('resolvedCount', () => {
  it('counts only conflicts that have a choice', () => {
    const text = lf(
      '<<<<<<< HEAD',
      'A',
      '=======',
      'B',
      '>>>>>>> other',
      'x',
      '<<<<<<< HEAD',
      'C',
      '=======',
      'D',
      '>>>>>>> other'
    )
    const sections = parseConflict(text)
    expect(resolvedCount(sections, {})).toBe(0)
    expect(resolvedCount(sections, { 0: 'ours' })).toBe(1)
    expect(resolvedCount(sections, { 0: 'ours', 1: 'theirs' })).toBe(2)
  })
})

describe('line-level picking', () => {
  /** A hunk whose sides each have several lines to choose among. */
  const MULTI = lf(
    'top',
    '<<<<<<< HEAD',
    'ours-1',
    'ours-2',
    '=======',
    'theirs-1',
    'theirs-2',
    '>>>>>>> other',
    'bottom'
  )

  it('writes only the picked lines', () => {
    const sections = parseConflict(MULTI)
    const choice = {
      kind: 'lines' as const,
      picks: [
        { side: 'ours' as const, index: 0 },
        { side: 'theirs' as const, index: 1 },
      ],
    }
    expect(renderSections(sections, { 0: choice })).toBe(
      lf('top', 'ours-1', 'theirs-2', 'bottom')
    )
  })

  /** Click order is the written order, so the user controls interleaving. */
  it('writes picks in the order they were made', () => {
    const sections = parseConflict(MULTI)
    const choice = {
      kind: 'lines' as const,
      picks: [
        { side: 'theirs' as const, index: 0 },
        { side: 'ours' as const, index: 0 },
      ],
    }
    expect(renderSections(sections, { 0: choice })).toBe(
      lf('top', 'theirs-1', 'ours-1', 'bottom')
    )
  })

  it('can take every line from both sides', () => {
    const sections = parseConflict(MULTI)
    const choice = {
      kind: 'lines' as const,
      picks: [
        { side: 'ours' as const, index: 0 },
        { side: 'ours' as const, index: 1 },
        { side: 'theirs' as const, index: 0 },
        { side: 'theirs' as const, index: 1 },
      ],
    }
    expect(renderSections(sections, { 0: choice })).toBe(
      lf('top', 'ours-1', 'ours-2', 'theirs-1', 'theirs-2', 'bottom')
    )
  })

  /** Picking nothing removes the region rather than leaving markers. */
  it('drops the region when nothing is picked', () => {
    const sections = parseConflict(MULTI)
    const choice = { kind: 'lines' as const, picks: [] }
    expect(renderSections(sections, { 0: choice })).toBe(lf('top', 'bottom'))
  })

  it('ignores a pick that points past the end of a side', () => {
    const sections = parseConflict(MULTI)
    const choice = {
      kind: 'lines' as const,
      picks: [
        { side: 'ours' as const, index: 0 },
        { side: 'theirs' as const, index: 99 },
      ],
    }
    expect(renderSections(sections, { 0: choice })).toBe(lf('top', 'ours-1', 'bottom'))
  })

  it('can pick from the common ancestor', () => {
    const sections = parseConflict(ZDIFF3)
    const choice = {
      kind: 'lines' as const,
      picks: [{ side: 'base' as const, index: 0 }],
    }
    expect(renderSections(sections, { 0: choice })).toBe(lf('line1', 'line2', 'line3'))
  })

  it('produces marker-free text', () => {
    const sections = parseConflict(MULTI)
    const choice = {
      kind: 'lines' as const,
      picks: [{ side: 'ours' as const, index: 0 }],
    }
    expect(hasMarkers(renderSections(sections, { 0: choice }))).toBe(false)
  })
})

describe('isDecided', () => {
  it('treats a whole-side choice as decided', () => {
    expect(isDecided('ours')).toBe(true)
    expect(isDecided('both-ours-first')).toBe(true)
  })

  it('treats no choice as undecided', () => {
    expect(isDecided(undefined)).toBe(false)
  })

  /** An empty selection is a decision in progress, not a decision. */
  it('treats an empty line selection as undecided', () => {
    expect(isDecided({ kind: 'lines', picks: [] })).toBe(false)
  })

  it('treats a non-empty line selection as decided', () => {
    expect(isDecided({ kind: 'lines', picks: [{ side: 'ours', index: 0 }] })).toBe(true)
  })
})

