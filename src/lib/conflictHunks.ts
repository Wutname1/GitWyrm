/**
 * Splitting a conflicted file into the parts that are contested and the parts
 * that are not.
 *
 * Git writes a conflicted file as ordinary text with marker lines in it. That
 * is a fine thing to hand to a text editor and a poor thing to hand to a
 * person: the interesting few lines are buried in a file that is otherwise
 * unchanged, and choosing a side means deleting the right marker lines by eye.
 * This module turns that text into a list of sections so the view can show only
 * what is actually in dispute, and rebuild the file from a choice per section.
 *
 * The shape it parses (`|||||||` base only appears in diff3/zdiff3 style):
 *
 *     <<<<<<< HEAD
 *     our version
 *     ||||||| 566a06a
 *     the common ancestor
 *     =======
 *     their version
 *     >>>>>>> other-branch
 *
 * Two details that look like nothing and are not:
 *
 * - **Line endings.** On a CRLF checkout the `\r` sits *inside* the marker
 *   line, so `line === '======='` is false for every marker in the file. Every
 *   comparison here is made against the line with its ending stripped, and the
 *   dominant ending is measured so the rebuilt file keeps the one it had.
 * - **Marker labels vary.** `<<<<<<< HEAD`, `||||||| 566a06a`,
 *   `>>>>>>> feature/x` -- the text after the run of angle brackets is a label,
 *   not part of the syntax. Only the seven-character prefix is matched.
 *
 * Malformed input is not an error. A file whose markers are unbalanced -- most
 * often because it was half hand-edited already -- parses as far as it can and
 * keeps the remainder as ordinary text. A conflict view that renders a corrupt
 * file as plain editable text is useful; one that throws is a blank screen at
 * the exact moment the user most needs to see their file.
 */

const OURS_MARKER = '<<<<<<<'
const BASE_MARKER = '|||||||'
const SPLIT_MARKER = '======='
const THEIRS_MARKER = '>>>>>>>'

/** A run of lines both sides agree on. */
export interface StableSection {
  kind: 'stable'
  lines: string[]
}

/** A run of lines the two sides disagree about. */
export interface ConflictSection {
  kind: 'conflict'
  /** Position among the conflicts only, so it survives edits to stable text. */
  id: number
  ours: string[]
  /** Common ancestor. Empty unless the markers were written in diff3 style. */
  base: string[]
  theirs: string[]
  /** Label after `<<<<<<<`, usually `HEAD`. */
  oursLabel: string
  /** Label after `>>>>>>>`, usually the incoming branch. */
  theirsLabel: string
  /** Label after `|||||||`, the ancestor's sha. Empty when there is no base. */
  baseLabel: string
}

export type Section = StableSection | ConflictSection

/**
 * One picked line, identified by the side it came from and its index there.
 *
 * Kept as a reference rather than the text itself so that the pick survives a
 * refetch, and so the order the user clicked in is the order written out.
 */
export interface LinePick {
  side: 'ours' | 'theirs' | 'base'
  /** Index within that side's lines. */
  index: number
}

/**
 * Which version of one conflict section to write out.
 *
 * The whole-side options settle the common cases in a click. `lines` is the
 * escape hatch for a conflict that is neither side wholesale: the user picks
 * individual lines from either version and they are written in the order
 * picked.
 */
export type Choice =
  | 'ours'
  | 'theirs'
  | 'base'
  | 'both-ours-first'
  | 'both-theirs-first'
  | { kind: 'lines'; picks: LinePick[] }

/** Choices so far, keyed by `ConflictSection.id`. */
export type Choices = Record<number, Choice | undefined>

/** The dominant line ending, so a rebuilt file keeps the one it arrived with. */
export type Eol = '\n' | '\r\n'

/** True when the line begins with `marker`, ignoring any trailing `\r`. */
function isMarker(line: string, marker: string): boolean {
  if (!line.startsWith(marker)) return false
  // `<<<<<<<` must not match `<<<<<<<<`: a longer run of the same character is
  // not this marker. Anything else (space, label, end of line) is fine.
  const next = line[marker.length]
  return next === undefined || next !== marker[0]
}

/** The label after a marker, e.g. `HEAD` in `<<<<<<< HEAD`. */
function labelOf(line: string, marker: string): string {
  return line.slice(marker.length).replace(/\r$/, '').trim()
}

/**
 * The line ending the file predominantly uses.
 *
 * Counted rather than sniffed from the first line, because a file with mixed
 * endings should be rebuilt with whichever one it mostly has -- rewriting every
 * line to match a stray first line would show the whole file as modified.
 */
export function detectEol(text: string): Eol {
  let crlf = 0
  let lf = 0
  for (let i = text.indexOf('\n'); i !== -1; i = text.indexOf('\n', i + 1)) {
    if (i > 0 && text[i - 1] === '\r') crlf++
    else lf++
  }
  return crlf > lf ? '\r\n' : '\n'
}

/** True if the text still contains any conflict marker line. */
export function hasMarkers(text: string): boolean {
  for (const raw of text.split('\n')) {
    const line = raw.replace(/\r$/, '')
    if (
      isMarker(line, OURS_MARKER) ||
      isMarker(line, BASE_MARKER) ||
      isMarker(line, SPLIT_MARKER) ||
      isMarker(line, THEIRS_MARKER)
    ) {
      return true
    }
  }
  return false
}

/**
 * Split conflicted text into stable and conflicted sections.
 *
 * Lines are stored without their endings; `renderSections` puts them back. A
 * file with no markers yields a single stable section, which is what lets the
 * same code path render an already-resolved file.
 */
export function parseConflict(text: string): Section[] {
  // A trailing newline would otherwise produce a phantom final empty line that
  // renderSections would then re-add, growing the file by a line each round
  // trip. Remembered here, restored there.
  const hadTrailingNewline = text.endsWith('\n')
  const body = hadTrailingNewline ? text.slice(0, -1) : text

  const lines = body.split('\n').map((l) => l.replace(/\r$/, ''))

  const sections: Section[] = []
  let stable: string[] = []
  let nextId = 0

  const flushStable = () => {
    if (stable.length > 0) {
      sections.push({ kind: 'stable', lines: stable })
      stable = []
    }
  }

  let i = 0
  while (i < lines.length) {
    if (!isMarker(lines[i], OURS_MARKER)) {
      stable.push(lines[i])
      i++
      continue
    }

    // Scan forward for the rest of this conflict. If any piece is missing the
    // markers are malformed, and we fall back to treating the opening marker as
    // ordinary text rather than swallowing the remainder of the file.
    const oursLabel = labelOf(lines[i], OURS_MARKER)
    const ours: string[] = []
    const base: string[] = []
    const theirs: string[] = []

    let j = i + 1
    let sawBase = false
    let sawSplit = false
    let closed = false
    let theirsLabel = ''
    let baseLabel = ''

    while (j < lines.length) {
      const line = lines[j]

      // A second `<<<<<<<` before this one closed means the first was never a
      // real conflict opener. Bail out and let it be re-read as stable text.
      if (isMarker(line, OURS_MARKER)) break

      if (!sawSplit && isMarker(line, BASE_MARKER)) {
        sawBase = true
        baseLabel = labelOf(line, BASE_MARKER)
        j++
        continue
      }
      if (isMarker(line, SPLIT_MARKER)) {
        sawSplit = true
        j++
        continue
      }
      if (isMarker(line, THEIRS_MARKER)) {
        theirsLabel = labelOf(line, THEIRS_MARKER)
        closed = true
        j++
        break
      }

      if (!sawSplit && sawBase) base.push(line)
      else if (!sawSplit) ours.push(line)
      else theirs.push(line)

      j++
    }

    if (!closed || !sawSplit) {
      // Malformed: keep the marker line as text and carry on from the line
      // after it, so a later well-formed conflict in the same file still parses.
      stable.push(lines[i])
      i++
      continue
    }

    flushStable()
    sections.push({
      kind: 'conflict',
      id: nextId++,
      ours,
      base,
      theirs,
      oursLabel,
      theirsLabel,
      baseLabel,
    })
    i = j
  }

  flushStable()

  // An empty file, or one that is entirely a trailing newline, still needs a
  // section so the editor has something to render.
  if (sections.length === 0) sections.push({ kind: 'stable', lines: [] })

  // Carried on the array so renderSections can restore it without re-parsing.
  ;(sections as SectionsMeta).trailingNewline = hadTrailingNewline
  return sections
}

/** Trailing-newline flag smuggled alongside the section list. */
interface SectionsMeta extends Array<Section> {
  trailingNewline?: boolean
}

/** The lines a conflict section contributes under a given choice. */
function linesFor(section: ConflictSection, choice: Choice | undefined): string[] {
  if (typeof choice === 'object' && choice.kind === 'lines') {
    // In click order, so a user assembling a line from both sides controls the
    // order rather than being given ours-then-theirs.
    return choice.picks
      .map(({ side, index }) => section[side][index])
      .filter((line): line is string => line !== undefined)
  }

  switch (choice) {
    case 'ours':
      return section.ours
    case 'theirs':
      return section.theirs
    case 'base':
      return section.base
    case 'both-ours-first':
      return [...section.ours, ...section.theirs]
    case 'both-theirs-first':
      return [...section.theirs, ...section.ours]
    default:
      // Undecided sections keep their markers, so a partially-resolved file
      // written to disk is still a valid conflicted file rather than a silent
      // pick of one side.
      return [
        `${OURS_MARKER} ${section.oursLabel}`.trimEnd(),
        ...section.ours,
        ...(section.base.length > 0
          ? [`${BASE_MARKER} ${section.baseLabel}`.trimEnd(), ...section.base]
          : []),
        SPLIT_MARKER,
        ...section.theirs,
        `${THEIRS_MARKER} ${section.theirsLabel}`.trimEnd(),
      ]
  }
}

/** Rebuild file text from sections and the choice made for each conflict. */
export function renderSections(
  sections: Section[],
  choices: Choices,
  eol: Eol = '\n'
): string {
  const out: string[] = []
  for (const section of sections) {
    if (section.kind === 'stable') out.push(...section.lines)
    else out.push(...linesFor(section, choices[section.id]))
  }

  const trailing = (sections as SectionsMeta).trailingNewline
  // Default to ending the file with a newline: it is what git, editors, and
  // POSIX all expect, and it is only omitted when the original omitted it.
  return out.join(eol) + (trailing === false ? '' : eol)
}

/** Every conflict section, in file order. */
export function conflictsOf(sections: Section[]): ConflictSection[] {
  return sections.filter((s): s is ConflictSection => s.kind === 'conflict')
}

/** How many conflicts have a choice, out of how many there are. */
export function resolvedCount(sections: Section[], choices: Choices): number {
  return conflictsOf(sections).filter((c) => isDecided(choices[c.id])).length
}

/**
 * True when a choice actually settles its conflict.
 *
 * A line selection with nothing picked yet is a decision in progress, not a
 * decision -- counting it would let "all resolved" appear while a hunk still
 * contributes nothing.
 */
export function isDecided(choice: Choice | undefined): boolean {
  if (choice === undefined) return false
  if (typeof choice === 'object') return choice.picks.length > 0
  return true
}
