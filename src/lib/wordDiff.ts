/**
 * Intra-line ("word") diff for the diff view.
 *
 * Git tells us a line was removed and another added, but not *what* inside the
 * line actually changed. When a long line has a one-character edit, the reader
 * has to eyeball both lines to spot it. This pairs each removed line with the
 * added line that replaced it and marks the differing spans so the view can
 * highlight them.
 */

export interface WordSpan {
  text: string
  /** True when this span differs from the paired line. */
  changed: boolean
}

/** Split into word-ish tokens: runs of identifier chars, whitespace, or single symbols. */
function tokenize(text: string): string[] {
  return text.match(/[A-Za-z0-9_$]+|\s+|[^A-Za-z0-9_$\s]/g) ?? []
}

function toSpans(tokens: string[], changed: boolean[]): WordSpan[] {
  const spans: WordSpan[] = []
  for (let i = 0; i < tokens.length; i++) {
    const last = spans[spans.length - 1]
    if (last && last.changed === changed[i]) last.text += tokens[i]
    else spans.push({ text: tokens[i], changed: changed[i] })
  }
  return spans
}

/**
 * Longest common subsequence over tokens, returning which tokens on each side
 * are part of the common run. Bounded so a pathological pair of very long lines
 * cannot stall rendering.
 */
const MAX_TOKENS = 400

function commonMask(a: string[], b: string[]): [boolean[], boolean[]] {
  const n = a.length
  const m = b.length
  const table: number[][] = Array.from({ length: n + 1 }, () => new Array<number>(m + 1).fill(0))
  for (let i = n - 1; i >= 0; i--) {
    for (let j = m - 1; j >= 0; j--) {
      table[i][j] = a[i] === b[j] ? table[i + 1][j + 1] + 1 : Math.max(table[i + 1][j], table[i][j + 1])
    }
  }
  const aCommon = new Array<boolean>(n).fill(false)
  const bCommon = new Array<boolean>(m).fill(false)
  let i = 0
  let j = 0
  while (i < n && j < m) {
    if (a[i] === b[j]) {
      aCommon[i] = true
      bCommon[j] = true
      i++
      j++
    } else if (table[i + 1][j] >= table[i][j + 1]) {
      i++
    } else {
      j++
    }
  }
  return [aCommon, bCommon]
}

/**
 * Compare a removed line against the added line that replaced it.
 * Returns null when the two lines share too little to make highlighting
 * meaningful — a wholly rewritten line reads better with no highlight at all
 * than with almost every character marked.
 */
export function diffWords(removed: string, added: string): { removed: WordSpan[]; added: WordSpan[] } | null {
  if (removed === added) return null
  const a = tokenize(removed)
  const b = tokenize(added)
  if (a.length === 0 || b.length === 0) return null
  if (a.length > MAX_TOKENS || b.length > MAX_TOKENS) return null

  const [aCommon, bCommon] = commonMask(a, b)

  // How much of the line survived, measured in characters rather than tokens so
  // whitespace-heavy lines are judged fairly.
  let sharedChars = 0
  for (let i = 0; i < a.length; i++) if (aCommon[i]) sharedChars += a[i].trim().length
  let totalChars = 0
  for (const t of a) totalChars += t.trim().length
  for (const t of b) totalChars += t.trim().length
  if (totalChars === 0) return null
  // sharedChars counts one side; compare against the average line length.
  if (sharedChars * 2 < totalChars * 0.25) return null

  return {
    removed: toSpans(a, aCommon.map((c) => !c)),
    added: toSpans(b, bCommon.map((c) => !c)),
  }
}

/** The subset of a diff line this module needs. */
interface SignedLine {
  sign: string
  text: string
}

/**
 * Walk a hunk's lines and word-diff each replaced line against its replacement.
 *
 * Git emits a replacement as a run of removed lines followed by a run of added
 * lines. We pair them by position within the run (1st removed with 1st added,
 * and so on). The runs need not be the same length: when one line is replaced
 * by many, the first added line is still an edit of the removed one and the
 * rest are new. Surplus lines on either side stay unpaired and render plain,
 * and `diffWords` independently drops any pair that turns out to share too
 * little to highlight honestly.
 *
 * Returns a map from line index to that line's spans; lines absent from the map
 * render as plain text.
 */
export function computeWordSpans(lines: SignedLine[]): Map<number, WordSpan[]> {
  const spans = new Map<number, WordSpan[]>()
  let i = 0
  while (i < lines.length) {
    if (lines[i].sign !== '-') {
      i++
      continue
    }
    const removedStart = i
    while (i < lines.length && lines[i].sign === '-') i++
    const addedStart = i
    while (i < lines.length && lines[i].sign === '+') i++
    const removedCount = addedStart - removedStart
    const addedCount = i - addedStart
    const pairCount = Math.min(removedCount, addedCount)
    for (let k = 0; k < pairCount; k++) {
      const rIndex = removedStart + k
      const aIndex = addedStart + k
      const result = diffWords(lines[rIndex].text, lines[aIndex].text)
      if (!result) continue
      spans.set(rIndex, result.removed)
      spans.set(aIndex, result.added)
    }
  }
  return spans
}
