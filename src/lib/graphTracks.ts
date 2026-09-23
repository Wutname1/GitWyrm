/**
 * Which graph column ("track") is free over a span of rows.
 *
 * The stash and WIP overlays are drawn in a column of their own, chosen by
 * walking outwards from a preferred lane until one is free. Answering "is this
 * track free between these rows" used to mean re-scanning every loaded row,
 * and it is asked from inside a `while (busy) track++` loop once per stash --
 * so the cost was rows x stashes x tracks. On a deeply scrolled graph that ran
 * into millions of iterations on every refetch, inside the window the
 * `graph.load` measurement covers.
 *
 * Indexing the rows once turns each probe into a scan of one track's own
 * spans. The occupancy rules are unchanged; see `trackIsBusy`.
 */

/** A stretch of one track that is already occupied. */
export interface TrackSpan {
  /** First row the span touches. */
  from: number
  /** Last row it touches. Equal to `from` for a commit node. */
  to: number
  /**
   * True for a commit's own node, false for an edge travelling to a parent.
   * The two are tested differently: a node only blocks a span it sits inside,
   * while an edge blocks any span it crosses.
   */
  isNode: boolean
}

/** The minimum a row has to expose for track occupancy to be worked out. */
export interface TrackRow {
  kind: string
  commit?: {
    sha: string
    lane: number
    parent_shas: string[]
    parent_lanes: (number | null | undefined)[]
  }
}

/**
 * Index every track's occupied spans in one pass.
 *
 * `rowBySha` locates a parent that is among the loaded rows. A parent that is
 * not loaded yet is treated as sitting on the next row down, matching how the
 * graph draws an edge that continues past the bottom of what is loaded.
 */
export function buildTrackSpans(
  rows: TrackRow[],
  rowBySha: Map<string, { row: number; lane: number }>
): Map<number, TrackSpan[]> {
  const byTrack = new Map<number, TrackSpan[]>()
  const add = (track: number, span: TrackSpan) => {
    const list = byTrack.get(track)
    if (list) list.push(span)
    else byTrack.set(track, [span])
  }
  for (let rowIndex = 0; rowIndex < rows.length; rowIndex++) {
    const row = rows[rowIndex]
    if (row.kind !== 'commit' || !row.commit) continue
    const commit = row.commit
    add(commit.lane, { from: rowIndex, to: rowIndex, isNode: true })
    for (let parentIndex = 0; parentIndex < commit.parent_shas.length; parentIndex++) {
      const parentSha = commit.parent_shas[parentIndex]
      const parentTrack = commit.parent_lanes[parentIndex] ?? commit.lane
      const parentRow = rowBySha.get(parentSha)?.row ?? rowIndex + 1
      add(parentTrack, { from: rowIndex, to: parentRow, isNode: false })
    }
  }
  return byTrack
}

/**
 * Is `track` occupied anywhere between `startRow` and `endRow`?
 *
 * A commit node blocks only a span it sits inside: `[startRow, endRow)`, so an
 * overlay may end exactly on the row of the commit it belongs to. An edge
 * blocks any span it crosses, but sharing an endpoint is allowed -- a stash
 * anchored to its base commit does not conflict with that commit's own edge.
 */
export function trackIsBusy(
  spans: Map<number, TrackSpan[]>,
  track: number,
  startRow: number,
  endRow: number
): boolean {
  const onTrack = spans.get(track)
  if (!onTrack) return false
  for (let i = 0; i < onTrack.length; i++) {
    const span = onTrack[i]
    if (span.isNode) {
      if (span.from >= startRow && span.from < endRow) return true
      continue
    }
    if (span.from < endRow && span.to > startRow) return true
  }
  return false
}
