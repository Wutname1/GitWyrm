/**
 * Line diff between two in-memory strings.
 *
 * This exists because the Rust diff commands compare git objects -- a blob
 * against a blob, or the index against the working tree. An AI-drafted spec edit
 * is neither: it is a string that has never been on disk, compared against a file
 * that has not been committed. Rather than teach the git layer about strings that
 * are not in git, the comparison happens here.
 *
 * The output is deliberately the same `{ sign, text }` shape the diff view
 * already renders, so `computeWordSpans` from `wordDiff.ts` can highlight what
 * changed inside a reworded line -- the case that matters most here, since a
 * drafted edit is usually a few sentences rewritten rather than lines added.
 */

export interface DiffLine {
  /** `' '` unchanged, `'-'` removed, `'+'` added. */
  sign: ' ' | '-' | '+'
  text: string
}

/**
 * Cap on the product of the two side lengths before falling back.
 *
 * The LCS table below is O(n*m) in both time and memory. Spec files are prose
 * and run to a few hundred lines, so this is never reached in practice; it is
 * here so that a pathological input degrades to a coarse diff instead of
 * freezing the window.
 */
const MAX_CELLS = 4_000_000

/**
 * Split into lines for diffing.
 *
 * A trailing newline would otherwise produce a final empty element that shows up
 * as a phantom changed line whenever one side ends with a newline and the other
 * does not. The newline still round-trips through the editor -- this only
 * affects how the comparison is displayed.
 */
function toLines(text: string): string[] {
  const normalized = text.replace(/\r\n/g, '\n')
  const lines = normalized.split('\n')
  if (lines.length > 1 && lines[lines.length - 1] === '') lines.pop()
  return lines
}

/**
 * Longest common subsequence over whole lines, walked back into a diff.
 *
 * Line-level LCS rather than a smarter algorithm because the inputs are small
 * and this is easy to be confident about: every line of both inputs appears
 * exactly once in the output, in order, which is the property that makes the
 * rendered diff trustworthy.
 */
export function diffLines(before: string, after: string): DiffLine[] {
  const a = toLines(before)
  const b = toLines(after)

  // Trim the common head and tail first. Real edits touch a small part of a
  // file, so this usually leaves an LCS table small enough to be free.
  let head = 0
  while (head < a.length && head < b.length && a[head] === b[head]) head++
  let tail = 0
  while (
    tail < a.length - head &&
    tail < b.length - head &&
    a[a.length - 1 - tail] === b[b.length - 1 - tail]
  ) {
    tail++
  }

  const aMid = a.slice(head, a.length - tail)
  const bMid = b.slice(head, b.length - tail)

  const out: DiffLine[] = []
  for (let i = 0; i < head; i++) out.push({ sign: ' ', text: a[i] })

  if (aMid.length * bMid.length > MAX_CELLS) {
    // Too large to align honestly: show the middle as a wholesale replacement
    // rather than spending seconds on a table nobody is waiting for.
    for (const text of aMid) out.push({ sign: '-', text })
    for (const text of bMid) out.push({ sign: '+', text })
  } else {
    const n = aMid.length
    const m = bMid.length
    // lcs[i][j] = length of the longest common subsequence of aMid[i..], bMid[j..]
    const lcs: number[][] = Array.from({ length: n + 1 }, () => new Array<number>(m + 1).fill(0))
    for (let i = n - 1; i >= 0; i--) {
      for (let j = m - 1; j >= 0; j--) {
        lcs[i][j] =
          aMid[i] === bMid[j] ? lcs[i + 1][j + 1] + 1 : Math.max(lcs[i + 1][j], lcs[i][j + 1])
      }
    }
    let i = 0
    let j = 0
    while (i < n && j < m) {
      if (aMid[i] === bMid[j]) {
        out.push({ sign: ' ', text: aMid[i] })
        i++
        j++
      } else if (lcs[i + 1][j] >= lcs[i][j + 1]) {
        out.push({ sign: '-', text: aMid[i] })
        i++
      } else {
        out.push({ sign: '+', text: bMid[j] })
        j++
      }
    }
    // Removals are emitted before additions so a replaced run reads as the
    // removed lines followed by their replacements -- the ordering
    // `computeWordSpans` pairs on.
    while (i < n) {
      out.push({ sign: '-', text: aMid[i] })
      i++
    }
    while (j < m) {
      out.push({ sign: '+', text: bMid[j] })
      j++
    }
  }

  for (let k = tail; k > 0; k--) out.push({ sign: ' ', text: a[a.length - k] })
  return out
}

/** How many lines the draft changes, for a plain-language summary. */
export function countChanges(lines: DiffLine[]): { added: number; removed: number } {
  let added = 0
  let removed = 0
  for (const line of lines) {
    if (line.sign === '+') added++
    else if (line.sign === '-') removed++
  }
  return { added, removed }
}

/** True when the draft is identical to what is already on disk. */
export function isUnchanged(lines: DiffLine[]): boolean {
  return lines.every((line) => line.sign === ' ')
}
