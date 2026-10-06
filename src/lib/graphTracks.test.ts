/**
 * The indexed track lookup replaced a predicate that re-scanned every loaded
 * row on every call. That was a pure performance change, so the bar for it is
 * that it answers identically -- not merely plausibly. The reference
 * implementation below is the original scan, kept verbatim, and the randomized
 * case asserts the two agree across thousands of probes on generated graphs.
 */
import { describe, expect, it } from 'vitest'
import { buildTrackSpans, trackIsBusy, type TrackRow } from './graphTracks'

/** The original O(rows) predicate, kept as the oracle. */
function referenceIsBusy(
  rows: TrackRow[],
  rowBySha: Map<string, { row: number; lane: number }>,
  track: number,
  startRow: number,
  endRow: number
): boolean {
  for (let rowIndex = 0; rowIndex < rows.length; rowIndex++) {
    const row = rows[rowIndex]
    if (row.kind !== 'commit' || !row.commit) continue
    const commit = row.commit
    if (rowIndex >= startRow && rowIndex < endRow && commit.lane === track) {
      return true
    }
    for (let parentIndex = 0; parentIndex < commit.parent_shas.length; parentIndex++) {
      const parentSha = commit.parent_shas[parentIndex]
      const parentTrack = commit.parent_lanes[parentIndex] ?? commit.lane
      if (parentTrack !== track) continue
      const parentRow = rowBySha.get(parentSha)?.row ?? rowIndex + 1
      if (rowIndex < endRow && parentRow > startRow) return true
    }
  }
  return false
}

function commitRow(sha: string, lane: number, parents: [string, number | null][]): TrackRow {
  return {
    kind: 'commit',
    commit: {
      sha,
      lane,
      parent_shas: parents.map(([p]) => p),
      parent_lanes: parents.map(([, l]) => l),
    },
  }
}

function indexRows(rows: TrackRow[]): Map<string, { row: number; lane: number }> {
  const m = new Map<string, { row: number; lane: number }>()
  rows.forEach((r, i) => {
    if (r.kind === 'commit' && r.commit) m.set(r.commit.sha, { row: i, lane: r.commit.lane })
  })
  return m
}

describe('trackIsBusy', () => {
  it('reports an empty track as free', () => {
    const rows = [commitRow('a', 0, [['b', 0]]), commitRow('b', 0, [])]
    const spans = buildTrackSpans(rows, indexRows(rows))
    expect(trackIsBusy(spans, 5, 0, 2)).toBe(false)
  })

  it('treats a commit node inside the span as occupying its lane', () => {
    const rows = [commitRow('a', 0, []), commitRow('b', 1, []), commitRow('c', 0, [])]
    const spans = buildTrackSpans(rows, indexRows(rows))
    expect(trackIsBusy(spans, 1, 1, 2)).toBe(true)
  })

  it('lets a span end exactly on a node row', () => {
    // endRow is exclusive for nodes: a stash anchored to its base commit must
    // not be pushed aside by that very commit.
    const rows = [commitRow('a', 0, []), commitRow('b', 1, [])]
    const spans = buildTrackSpans(rows, indexRows(rows))
    expect(trackIsBusy(spans, 1, 0, 1)).toBe(false)
  })

  it('treats an edge crossing the span as occupying the track', () => {
    // 'a' at row 0 has a parent at row 3 on track 2, so it crosses rows 1-2.
    const rows = [
      commitRow('a', 0, [['d', 2]]),
      commitRow('b', 1, []),
      commitRow('c', 1, []),
      commitRow('d', 2, []),
    ]
    const spans = buildTrackSpans(rows, indexRows(rows))
    expect(trackIsBusy(spans, 2, 1, 2)).toBe(true)
  })

  it('allows an edge that only shares an endpoint', () => {
    const rows = [commitRow('a', 0, [['b', 0]]), commitRow('b', 0, [])]
    const spans = buildTrackSpans(rows, indexRows(rows))
    // The edge runs 0 -> 1; a span ending at row 0 only touches its start.
    expect(trackIsBusy(spans, 0, -1, 0)).toBe(false)
  })

  it('places an edge to an unloaded parent on the row below', () => {
    // No row for 'z', so its edge is treated as continuing one row down --
    // that is how the graph draws history that has not paged in yet.
    const rows = [commitRow('a', 3, [['z', 3]])]
    const spans = buildTrackSpans(rows, indexRows(rows))
    expect(trackIsBusy(spans, 3, 0, 1)).toBe(true)
  })

  it('falls back to the commit lane when a parent lane is missing', () => {
    const rows = [commitRow('a', 4, [['b', null]]), commitRow('b', 4, [])]
    const spans = buildTrackSpans(rows, indexRows(rows))
    expect(trackIsBusy(spans, 4, 0, 2)).toBe(true)
  })

  it('ignores non-commit rows', () => {
    const rows: TrackRow[] = [{ kind: 'wip' }, { kind: 'stash' }, commitRow('a', 0, [])]
    const spans = buildTrackSpans(rows, indexRows(rows))
    expect(trackIsBusy(spans, 0, 0, 2)).toBe(false)
  })

  it('answers identically to the original row scan on random graphs', () => {
    // The real guarantee. Deterministic PRNG so a failure is reproducible.
    let seed = 0x5eed
    const rand = (n: number) => {
      seed = (seed * 1664525 + 1013904223) >>> 0
      return seed % n
    }

    for (let trial = 0; trial < 40; trial++) {
      const size = 5 + rand(45)
      const rows: TrackRow[] = []
      for (let i = 0; i < size; i++) {
        if (rand(6) === 0) {
          rows.push({ kind: rand(2) === 0 ? 'wip' : 'stash' })
          continue
        }
        const parentCount = rand(3)
        const parents: [string, number | null][] = []
        for (let p = 0; p < parentCount; p++) {
          // Mostly real parents further down; sometimes one never loaded.
          const target = i + 1 + rand(8)
          const sha = target < size ? `c${target}` : `missing${rand(3)}`
          parents.push([sha, rand(4) === 0 ? null : rand(5)])
        }
        rows.push(commitRow(`c${i}`, rand(5), parents))
      }

      const byySha = indexRows(rows)
      const spans = buildTrackSpans(rows, byySha)
      for (let track = 0; track < 6; track++) {
        for (let start = 0; start < size; start += 3) {
          for (let end = start; end <= size; end += 3) {
            expect(trackIsBusy(spans, track, start, end)).toBe(
              referenceIsBusy(rows, byySha, track, start, end)
            )
          }
        }
      }
    }
  })
})
