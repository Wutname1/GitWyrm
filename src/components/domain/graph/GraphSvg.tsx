import { useCallback, useEffect, useMemo } from 'react'
import type { CommitEntry, StashInfo } from '@/lib/bindings'
import { laneColor } from '@/lib/gitDisplay'
import { useUiStore } from '@/stores/uiStore'

const X0 = 14
const LANE_WIDTH = 20

/**
 * Route an edge like a rail line: change lanes close to an endpoint, then run
 * vertically through time. Long diagonal curves are hard to follow once two
 * branches overlap, while short bends keep each lane visually stable.
 */
function railPath(
  xStart: number,
  yStart: number,
  xTrack: number,
  xEnd: number,
  yEnd: number,
  rowHeight: number,
): string {
  if (xStart === xTrack && xTrack === xEnd) {
    return `M ${xStart} ${yStart} L ${xEnd} ${yEnd}`
  }

  const radius = Math.min(9, rowHeight * 0.34, Math.max(4, (yEnd - yStart) / 3))
  const commands = [`M ${xStart} ${yStart}`]

  if (xStart !== xTrack) {
    commands.push(
      `C ${xStart} ${yStart + radius}, ${xTrack} ${yStart + radius}, ${xTrack} ${yStart + radius * 2}`,
    )
  }

  const trackStartY = xStart === xTrack ? yStart : yStart + radius * 2
  const trackEndY = xTrack === xEnd ? yEnd : yEnd - radius * 2
  if (trackEndY > trackStartY) commands.push(`L ${xTrack} ${trackEndY}`)

  if (xTrack !== xEnd) {
    commands.push(
      `C ${xTrack} ${yEnd - radius}, ${xEnd} ${yEnd - radius}, ${xEnd} ${yEnd}`,
    )
  }

  return commands.join(' ')
}

function overlayPath(
  x: number,
  y: number,
  xBase: number,
  yBase: number,
  rowHeight: number,
): string {
  if (x === xBase) return `M ${x} ${y} L ${xBase} ${yBase}`
  const bendY = Math.max(y, yBase - rowHeight)
  return `M ${x} ${y} L ${x} ${bendY} C ${x} ${bendY + (yBase - bendY) * 0.5}, ${xBase} ${bendY + (yBase - bendY) * 0.5}, ${xBase} ${yBase}`
}

/** One row of the graph: the synthetic WIP row, a stash, or a real commit. */
export type GraphRow =
  | { kind: 'wip' }
  | { kind: 'stash'; stash: StashInfo }
  | { kind: 'commit'; commit: CommitEntry }

interface GraphSvgProps {
  rows: GraphRow[]
  selectedSha: string | null
  /** Virtualized visible window (inclusive row indices). */
  startIndex: number
  endIndex: number
  /** Current width of the graph grid cell. */
  width: number
  /** Height shared by the virtualized rows and graph geometry. */
  rowHeight: number
}

/**
 * Renders edges from each commit to its parents using the backend-computed
 * lanes. Edge endpoints use the parent's row index (found via sha lookup);
 * parents beyond the loaded pages get a short fading stub. The WIP row gets a
 * dashed node in the HEAD commit's lane; each stash row gets an archive-box
 * node in its base commit's lane with a dashed edge down to that commit, so a
 * stash reads as saved work attached to this point in history, not a commit.
 */
export function GraphSvg({ rows, selectedSha, startIndex, endIndex, width, rowHeight }: GraphSvgProps) {
  const rowCenterY = (row: number) => row * rowHeight + rowHeight / 2

  // Row index and lane of every loaded commit, keyed by sha.
  const commitRowBySha = useMemo(() => {
    const m = new Map<string, { row: number; lane: number }>()
    rows.forEach((r, i) => {
      if (r.kind === 'commit') m.set(r.commit.sha, { row: i, lane: r.commit.lane })
    })
    return m
  }, [rows])

  const commitTrackIsBusy = useMemo(
    () => (track: number, startRow: number, endRow: number) => {
      for (let rowIndex = 0; rowIndex < rows.length; rowIndex++) {
        const row = rows[rowIndex]
        if (row.kind !== 'commit') continue

        // A commit node inside the overlay's span owns this lane even if its
        // incoming edge happens to travel through another track.
        if (rowIndex >= startRow && rowIndex < endRow && row.commit.lane === track) {
          return true
        }

        for (let parentIndex = 0; parentIndex < row.commit.parent_shas.length; parentIndex++) {
          const parentSha = row.commit.parent_shas[parentIndex]
          const parentTrack = row.commit.parent_lanes[parentIndex] ?? row.commit.lane
          if (parentTrack !== track) continue
          const parentRow = commitRowBySha.get(parentSha)?.row ?? rowIndex + 1
          // Sharing the base endpoint is fine; crossing the span is not.
          if (rowIndex < endRow && parentRow > startRow) return true
        }
      }
      return false
    },
    [rows, commitRowBySha],
  )

  // The WIP row is a synthetic child of the checked-out branch tip, not of
  // whichever time-sorted commit happens to appear first.
  const headCommit = useMemo(() => {
    for (let row = 0; row < rows.length; row++) {
      const item = rows[row]
      if (item.kind === 'commit' && item.commit.refs.some((ref) => ref.type === 'head')) {
        return { row, lane: item.commit.lane }
      }
    }
    return null
  }, [rows])

  const showPending = rows[0]?.kind === 'wip' && startIndex === 0
  const pendingTrack = useMemo(() => {
    if (!showPending || headCommit == null) return null
    let track = headCommit.lane
    while (commitTrackIsBusy(track, 0, headCommit.row)) track++
    return track
  }, [showPending, headCommit, commitTrackIsBusy])

  // Column assignment for stash tracks. A stash directly above its base stays
  // in that commit's lane: there is no intervening history to route around.
  // Longer or overlapping stash spans get separate side tracks so they remain
  // readable instead of looking like one continuous line.
  const stashTrackBySha = useMemo(() => {
    const m = new Map<string, number>()
    const active: { endRow: number; track: number }[] =
      pendingTrack != null && headCommit != null
        ? [{ endRow: headCommit.row, track: pendingTrack }]
        : []

    rows.forEach((r, i) => {
      if (r.kind !== 'stash') return
      const base = commitRowBySha.get(r.stash.base_sha)
      const baseLane = base?.lane ?? 0
      const endRow = base?.row ?? i + 1
      // Prefer the actual base lane regardless of how many unrelated rows sit
      // between the stash and that commit. Move outward only when a real edge
      // or another stash already occupies the span.
      let track = base == null ? baseLane + 1 : baseLane
      while (
        commitTrackIsBusy(track, i, endRow) ||
        active.some((a) => a.track === track && a.endRow >= i)
      ) {
        track++
      }
      active.push({ endRow, track })
      m.set(r.stash.sha, track)
    })
    return m
  }, [rows, commitRowBySha, commitTrackIsBusy, pendingTrack, headCommit])

  // Share the assignment so the sidebar can tint its stash icon to match the
  // node drawn here. The store de-dupes, so republishing an unchanged map on
  // every scroll or paging render does not re-render its readers.
  const setStashTracks = useUiStore((s) => s.setStashTracks)
  useEffect(() => {
    setStashTracks(Object.fromEntries(stashTrackBySha))
  }, [stashTrackBySha, setStashTracks])

  // Keep every lane inside the resized graph cell. At the default and wider
  // sizes lanes retain their familiar 20px rhythm; narrowing the column
  // compresses only the horizontal spacing, never the row or edge topology.
  const maxTrack = useMemo(() => {
    let max = pendingTrack ?? 0
    for (const row of rows) {
      if (row.kind !== 'commit') continue
      max = Math.max(max, row.commit.lane, ...row.commit.parent_lanes)
    }
    for (const track of stashTrackBySha.values()) max = Math.max(max, track)
    return max
  }, [rows, pendingTrack, stashTrackBySha])
  const laneSpacing = maxTrack === 0
    ? LANE_WIDTH
    : Math.min(LANE_WIDTH, (width - X0 * 2) / maxTrack)
  const laneX = useCallback((lane: number) => X0 + lane * laneSpacing, [laneSpacing])

  const edges = useMemo(() => {
    const out: { d: string; color: string; fade?: boolean; dashed?: boolean }[] = []
    const lo = Math.max(0, startIndex - 30)
    const hi = Math.min(rows.length - 1, endIndex + 30)
    for (let i = lo; i <= hi; i++) {
      const r = rows[i]
      if (r.kind !== 'commit') continue
      const c = r.commit
      c.parent_shas.forEach((parentSha, pi) => {
        const parent = commitRowBySha.get(parentSha)
        const x1 = laneX(c.lane)
        const y1 = rowCenterY(i)
        const parentLane = c.parent_lanes[pi] ?? c.lane
        if (parent == null) {
          // Parent not loaded yet: draw a stub downward.
          out.push({
            d: `M ${x1} ${y1} L ${laneX(parentLane)} ${y1 + rowHeight}`,
            color: laneColor(parentLane),
            fade: true,
          })
          return
        }
        const xTrack = laneX(parentLane)
        const x2 = laneX(parent.lane)
        const y2 = rowCenterY(parent.row)
        out.push({
          d: railPath(x1, y1, xTrack, x2, y2, rowHeight),
          color: laneColor(parentLane),
        })
      })
    }
    return out
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rows, commitRowBySha, startIndex, endIndex, rowHeight, laneX])

  return (
    <svg
      width={width}
      height={rows.length * rowHeight}
      className="block overflow-hidden"
    >
      {edges.map((e, i) => (
        <path
          key={i}
          d={e.d}
          fill="none"
          stroke={e.color}
          strokeWidth={2.25}
          strokeLinecap="round"
          opacity={e.fade ? 0.35 : 1}
        />
      ))}
      {showPending && (
        <>
          {headCommit != null && pendingTrack != null && (
            <path
              d={overlayPath(
                laneX(pendingTrack),
                rowCenterY(0),
                laneX(headCommit.lane),
                rowCenterY(headCommit.row),
                rowHeight,
              )}
              fill="none"
              stroke={laneColor(pendingTrack)}
              strokeWidth={2.25}
              strokeLinecap="round"
              strokeDasharray="2 4"
              opacity={0.7}
            />
          )}
          <circle
            cx={laneX(pendingTrack ?? 0)}
            cy={rowCenterY(0)}
            r={6}
            fill="var(--gw-bg)"
            stroke={laneColor(pendingTrack ?? 0)}
            strokeWidth={2}
            strokeDasharray="2.5 2.5"
          />
        </>
      )}
      {rows.map((r, i) => {
        if (i < startIndex - 30 || i > endIndex + 30) return null
        if (r.kind === 'stash') {
          // A stash immediately above its base commit stays in that lane. If
          // time-sorted history sits between them, it moves to a side track so
          // the dashed connector can pass those rows without hiding a branch.
          const base = commitRowBySha.get(r.stash.base_sha)
          const sel = selectedSha === r.stash.sha
          // Longer overlapping stash spans stagger into further columns. The
          // track also picks the color, keeping neighboring spans distinct.
          const track = stashTrackBySha.get(r.stash.sha) ?? (base?.lane ?? 0) + 1
          const x = laneX(track)
          const y = rowCenterY(i)
          const col = laneColor(track)
          return (
            <g key={`stash:${r.stash.sha}`}>
              {base != null ? (
                // Straight drop in the stash track, then a one-row bend into
                // the base commit -- the same elbow shape commit edges use.
                <path
                  d={(() => {
                    const xBase = laneX(base.lane)
                    const yBase = rowCenterY(base.row)
                    if (track === base.lane) {
                      return `M ${x} ${y} L ${xBase} ${yBase}`
                    }
                    return overlayPath(x, y, xBase, yBase, rowHeight)
                  })()}
                  fill="none"
                  stroke={col}
                  strokeWidth={2}
                  strokeLinecap="round"
                  strokeDasharray="2 4"
                  opacity={sel ? 0.9 : 0.5}
                />
              ) : (
                // Base commit not paged in yet: fading stub, like unloaded parents.
                <path
                  d={`M ${x} ${y} L ${x} ${y + rowHeight}`}
                  fill="none"
                  stroke={col}
                  strokeWidth={2}
                  strokeLinecap="round"
                  strokeDasharray="2 4"
                  opacity={0.3}
                />
              )}
              <g
                transform={`translate(${x} ${y})`}
                fill="var(--gw-bg)"
                stroke={sel ? 'var(--gw-text)' : col}
                strokeWidth={sel ? 2.25 : 1.75}
                strokeLinecap="round"
                strokeLinejoin="round"
                opacity={sel ? 1 : 0.9}
              >
                <rect
                  x={sel ? -7.5 : -7}
                  y={sel ? -6.5 : -6}
                  width={sel ? 15 : 14}
                  height={sel ? 13 : 12}
                  rx={2}
                />
                <path d={`M ${sel ? -7.5 : -7} -2 H ${sel ? 7.5 : 7}`} />
                <path d="M -2 1 H 2" />
              </g>
            </g>
          )
        }
        if (r.kind !== 'commit') return null
        if (i < startIndex || i > endIndex) return null
        const c = r.commit
        const sel = selectedSha === c.sha
        const col = laneColor(c.lane)
        return (
          <circle
            key={c.sha}
            cx={laneX(c.lane)}
            cy={rowCenterY(i)}
            r={sel ? 7.5 : 6}
            fill={c.is_merge ? 'var(--gw-bg)' : col}
            stroke={sel ? 'var(--gw-text)' : c.is_merge ? col : 'var(--gw-bg)'}
            strokeWidth={sel ? 2.5 : c.is_merge ? 2.5 : 2}
          />
        )
      })}
    </svg>
  )
}
