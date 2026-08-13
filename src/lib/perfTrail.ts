/**
 * A small ring of recent timings, kept so a bug report can say how slow things
 * actually were.
 *
 * Sentry spans answer this in aggregate, but a single user reporting "it is
 * taking a long time to open repos for the first time" arrives with no numbers
 * at all: the spans are in a different dataset, keyed by nothing that ties them
 * to the report. Recording the last few durations in memory and attaching them
 * to the feedback event closes that gap -- the report itself says which phase
 * was slow, on that machine, for that repo.
 *
 * Deliberately tiny and in-memory: this is diagnostic seasoning on a report the
 * user chose to send, not analytics. Nothing is persisted, nothing is sent
 * unless a report is submitted, and no paths or branch names are recorded --
 * only a label, a duration, and optional plain scalars.
 */

/** One recorded timing. */
export interface PerfMark {
  /** What was measured, e.g. `openRepo` or `watch.arm`. */
  label: string
  /** How long it took, in milliseconds, rounded to whole ms. */
  ms: number
  /** Milliseconds before now that it finished, filled in when read. */
  agoMs: number
  /** Extra scalars worth keeping, e.g. `{ cold: true }`. Never free text. */
  detail?: Record<string, string | number | boolean>
}

interface StoredMark extends Omit<PerfMark, 'agoMs'> {
  at: number
}

/**
 * Enough to cover an app start plus a few repo opens, and small enough that the
 * attachment stays readable. Oldest entries fall off the end.
 */
const MAX_MARKS = 40

const marks: StoredMark[] = []

/**
 * Record how long something took.
 *
 * Durations are rounded to whole milliseconds: sub-millisecond precision says
 * nothing about felt latency and only makes the report harder to scan.
 */
export function recordPerfMark(
  label: string,
  ms: number,
  detail?: Record<string, string | number | boolean>
): void {
  marks.push({ label, ms: Math.round(ms), at: Date.now(), detail })
  if (marks.length > MAX_MARKS) marks.splice(0, marks.length - MAX_MARKS)
}

/** Times `fn`, records the result under `label`, and returns what it returned. */
export async function timed<T>(
  label: string,
  fn: () => Promise<T>,
  detail?: Record<string, string | number | boolean>
): Promise<T> {
  const start = performance.now()
  try {
    return await fn()
  } finally {
    recordPerfMark(label, performance.now() - start, detail)
  }
}

/** The recorded marks, newest last, with `agoMs` resolved against now. */
export function readPerfMarks(): PerfMark[] {
  const now = Date.now()
  return marks.map(({ at, ...m }) => ({ ...m, agoMs: now - at }))
}

/**
 * The marks as a plain-text block for a bug report.
 *
 * Returns '' when nothing has been recorded, so a caller can leave the section
 * out entirely rather than attach an empty heading.
 */
export function perfMarksText(): string {
  const all = readPerfMarks()
  if (all.length === 0) return ''
  return all
    .map((m) => {
      const detail = m.detail
        ? ` ${Object.entries(m.detail)
            .map(([k, v]) => `${k}=${v}`)
            .join(' ')}`
        : ''
      return `${(m.agoMs / 1000).toFixed(1)}s ago  ${m.label}  ${m.ms}ms${detail}`
    })
    .join('\n')
}

/** Drops every recorded mark. Test-only; the app has no reason to forget. */
export function resetPerfMarks(): void {
  marks.length = 0
}
