import { useEffect, useId, useMemo } from 'react'
import type { CommitEntry, StashInfo } from '@/lib/bindings'
import { laneColor } from '@/lib/gitDisplay'
import { laneGeometry } from '@/lib/graphLanes'
import { useAvatarUrls } from '@/lib/useAvatarUrls'
import { useUiStore } from '@/stores/uiStore'
import { useWorkspaceStore } from '@/stores/workspaceStore'

/**
 * Geometry of an avatar node: a 22px node made of an 18px picture inside a 2px
 * ring, which is the size a face stays recognisable at.
 *
 * The ring sits flush on the picture rather than separated from it. A gap was
 * tried and cost 6px of face to buy separation the extra size provides anyway.
 *
 * 22px is wider than the 20px lane pitch, so `AVATAR_LANE_WIDTH` widens the
 * pitch whenever avatars are on -- otherwise adjacent lanes would overlap.
 */
const AVATAR_OUTER_R = 11
/** Width of the lane-colored ring. */
const AVATAR_RING = 2
/** Radius of the picture itself. */
const AVATAR_R = AVATAR_OUTER_R - AVATAR_RING
/** Lane pitch while avatars are on: the node's width plus breathing room. */
const AVATAR_LANE_WIDTH = AVATAR_OUTER_R * 2 + 2

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
  /**
   * More history is still to load. A parent that is not among the loaded rows
   * is then simply further down the walk, so its lane is drawn continuing off
   * the bottom rather than stopping dead.
   */
  hasMorePages?: boolean
}

/**
 * Renders edges from each commit to its parents using the backend-computed
 * lanes. Edge endpoints use the parent's row index (found via sha lookup);
 * parents beyond the loaded pages get a short fading stub. The WIP row gets a
 * dashed node in the HEAD commit's lane; each stash row gets an archive-box
 * node in its base commit's lane with a dashed edge down to that commit, so a
 * stash reads as saved work attached to this point in history, not a commit.
 */
export function GraphSvg({ rows, selectedSha, startIndex, endIndex, width, rowHeight, hasMorePages = false }: GraphSvgProps) {
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

  const showAvatars = useWorkspaceStore((s) => s.showGraphAvatars)

  // Only the authors actually on screen are resolved. Walking every loaded row
  // would fire hundreds of lookups the moment a large repo finishes paging,
  // for faces that are nowhere near the viewport.
  const visibleEmails = useMemo(() => {
    if (!showAvatars) return []
    const out: string[] = []
    const lo = Math.max(0, startIndex - 30)
    const hi = Math.min(rows.length - 1, endIndex + 30)
    for (let i = lo; i <= hi; i++) {
      const r = rows[i]
      if (r.kind === 'commit') out.push(r.commit.author_email)
    }
    return out
  }, [showAvatars, rows, startIndex, endIndex])

  // Doubled for crisp rendering on high-DPI displays, the same trade `Avatar`
  // makes for the author column.
  const avatarUrls = useAvatarUrls(visibleEmails, Math.round(AVATAR_R * 4))

  // Clip paths are referenced by id, so two graphs on screen at once (a diff
  // view beside the log) must not collide on the same names. React's ids are
  // wrapped in colons, which are legal in an id but not in the `url(#...)`
  // reference that reads it back, so they are stripped.
  const clipPrefix = useId().replace(/:/g, '')

  // Lanes keep a fixed width so a branch sits in the same column no matter how
  // wide the graph is, and widening the column reveals more lanes instead of
  // re-spacing the ones already drawn. Lanes that do not fit are folded onto
  // the last visible column rather than compressing every lane to fit; that
  // column then reads as "and more branches out here", which stays legible
  // where 20-odd hairline rails would not.
  // Avatar nodes are wider than the default pitch, so the pitch widens with
  // them. Turning avatars on therefore folds a few more lanes into the
  // overflow column at the same graph width, which is the trade for a node big
  // enough to recognise a face in.
  const { laneX, isOverflow } = useMemo(
    () => laneGeometry(width, showAvatars ? AVATAR_LANE_WIDTH : undefined),
    [width, showAvatars],
  )

  // Rows occupied by each lane, ascending, so a lane running off the loaded
  // region can stop before reaching a commit that is not its parent. Built once
  // per row set rather than rescanned for every unresolved edge.
  const rowsByLane = useMemo(() => {
    const byLane = new Map<number, number[]>()
    rows.forEach((r, i) => {
      if (r.kind !== 'commit') return
      const list = byLane.get(r.commit.lane)
      if (list) list.push(i)
      else byLane.set(r.commit.lane, [i])
    })
    return byLane
  }, [rows])

  /**
   * Y to run a lane down to when its parent has not been paged in.
   *
   * Reaches the next commit occupying the same lane, because a lane is one line
   * of history: that commit is where this line continues, even though the
   * immediate parent is on a page that has not loaded. Stopping short of it
   * instead left a gap in the middle of a lane with nothing to fill it.
   *
   * With no later commit in the lane, the line runs off the loaded rows -- the
   * branch really does carry on below.
   */
  const laneContinuesToY = (lane: number, fromRow: number) => {
    const next = rowsByLane.get(lane)?.find((row) => row > fromRow)
    if (next == null) return rows.length * rowHeight
    return rowCenterY(next)
  }

  // How far above the window a commit can sit and still have an edge reaching
  // into it. An edge is drawn when its CHILD is iterated, so a child further
  // above than this leaves a gap where its rail should cross the viewport.
  // Covers the long off-page lanes, which are the ones that span most rows.
  const longestEdgeSpan = useMemo(() => {
    let longest = 0
    rows.forEach((r, i) => {
      if (r.kind !== 'commit') return
      for (const parentSha of r.commit.parent_shas) {
        const parentRow = commitRowBySha.get(parentSha)?.row
        if (parentRow != null && parentRow - i > longest) longest = parentRow - i
      }
    })
    return longest
  }, [rows, commitRowBySha])

  const edges = useMemo(() => {
    const out: { d: string; color: string; fade?: boolean; dashed?: boolean }[] = []
    // Reach back far enough to catch every edge whose child is above the
    // viewport but whose rail still crosses it. Without this, a long edge
    // simply vanishes and leaves a gap in the middle of an otherwise solid
    // lane. Bounded by the longest edge actually present, so a graph of short
    // edges still only scans a little beyond the window.
    const lo = Math.max(0, startIndex - Math.max(30, longestEdgeSpan))
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
          // The parent is real but has not been paged in. Parent lookup only
          // covers loaded rows, and a branch tip near the top of the graph can
          // easily have its parent hundreds of rows -- several pages -- below,
          // so this is common rather than exceptional.
          //
          // The lane genuinely continues down there, so it is drawn running to
          // the end of the loaded rows: "this branch carries on below" is true,
          // and it reads as a lane rather than a severed stub.
          //
          // It follows the lane the BACKEND reserved for this parent, which for
          // a merge's second parent is a lane of its own, not the merge's. That
          // lane is already holding a column open, so drawing nothing there left
          // a reserved-but-empty gap -- visible as a run of merge commits with
          // space beside them and no line in it.
          //
          // With everything loaded, an unresolved parent instead means the
          // commit is outside the walk (a graft, or a ref excluded from it),
          // and there is nothing below to point at.
          //
          // The run reaches the next commit in that lane, which is where the
          // line continues once the missing page arrives. It is drawn solid:
          // the lane really is continuous there, and fading it made a real
          // connection look like a loading artifact.
          if (hasMorePages) {
            const xParent = laneX(parentLane)
            // Step across into the parent's lane first when it differs, so the
            // line leaves the merge node instead of appearing beside it.
            const d =
              parentLane === c.lane
                ? `M ${x1} ${y1} L ${x1} ${laneContinuesToY(c.lane, i)}`
                : railPath(x1, y1, xParent, xParent, laneContinuesToY(parentLane, i), rowHeight)
            out.push({ d, color: laneColor(parentLane) })
          }
          return
        }
        // Every overflowing lane clamps to the same x, so a rail drawn into
        // that column would visually terminate at whatever unrelated commit
        // happens to sit there -- a line that appears to connect two commits
        // that have no relationship. Only the endpoint actually inside the
        // column is drawn, as a short stub reading outward.
        if (isOverflow(parent.lane) !== isOverflow(c.lane)) {
          const inside = isOverflow(c.lane) ? parent : { row: i, lane: c.lane }
          const xInside = laneX(inside.lane)
          const yInside = rowCenterY(inside.row)
          // Vertical, in the drawable end's own lane. A horizontal reach toward
          // the overflow column would point at a column shared by many
          // branches, which is exactly the false connection being avoided.
          const towardParent = isOverflow(c.lane) ? -1 : 1
          out.push({
            d: `M ${xInside} ${yInside} L ${xInside} ${yInside + towardParent * rowHeight * 0.6}`,
            color: laneColor(inside.lane),
            fade: true,
          })
          return
        }
        // Both ends out in the overflow column: the whole edge lives somewhere
        // we cannot draw truthfully, so nothing is drawn rather than a rail
        // between two commits that only share a clamped column.
        if (isOverflow(c.lane)) return

        // Both endpoints are visible, but the track the edge would travel down
        // is clamped. Route it along the parent's own lane instead, so the rail
        // never detours through the shared overflow column.
        const track = isOverflow(parentLane) ? parent.lane : parentLane
        const xTrack = laneX(track)
        const x2 = laneX(parent.lane)
        const y2 = rowCenterY(parent.row)
        out.push({
          d: railPath(x1, y1, xTrack, x2, y2, rowHeight),
          color: laneColor(track),
        })
      })
    }
    return out
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rows, commitRowBySha, startIndex, endIndex, rowHeight, laneX, isOverflow, hasMorePages, longestEdgeSpan])

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
        // Commits whose lane folded into the overflow column are drawn smaller
        // and hollow, so a stack of them reads as "more branches out here"
        // rather than as several unrelated branches sharing one column.
        const overflow = isOverflow(c.lane)
        const x = laneX(c.lane)
        const y = rowCenterY(i)

        // A picture only replaces the dot once it has actually resolved.
        // Swapping in an empty disc first would flash a hole in the lane on
        // every scroll, and authors with no picture anywhere never resolve at
        // all -- they keep the plain dot for good.
        //
        // Overflow commits stay dots regardless: at 3.5px a face is a smudge,
        // and shrinking that column is what makes it read as "more out here".
        const avatar = showAvatars && !overflow ? avatarUrls.get(c.author_email.trim().toLowerCase()) : undefined
        if (avatar) {
          const clipId = `${clipPrefix}-${c.sha}`
          const maskId = `${clipPrefix}-m${c.sha}`
          // The stroke straddles its path, so the ring is centred half a width
          // inside the outer edge to keep the node at its stated size.
          const ringR = AVATAR_OUTER_R - AVATAR_RING / 2
          // A tool mark is a flat glyph, not a photo: it has no background of
          // its own and does not fill a circle. It gets a dark disc to sit on
          // -- the same treatment the author column gives it -- and is inset so
          // the glyph is not cropped by the round clip.
          const glyphR = avatar.bot ? AVATAR_R * 0.62 : AVATAR_R
          return (
            <g key={c.sha}>
              <defs>
                <clipPath id={clipId}>
                  <circle cx={x} cy={y} r={AVATAR_R} />
                </clipPath>
                {avatar.mono && (
                  // A silhouette is drawn as `currentColor`, which inside an
                  // <image> resolves against that image's own document and
                  // comes out black -- invisible here. Used as a mask instead,
                  // the art is only a stencil and the fill is ours.
                  //
                  // `mask-type="alpha"` is what makes that work. An SVG mask
                  // defaults to *luminance*, so it weighs how bright the art is
                  // -- and this art is solid black, luminance zero, which masks
                  // everything away and leaves an empty dark disc. The shape we
                  // want is carried entirely by the alpha channel: opaque where
                  // the glyph is, transparent everywhere else. Reading alpha
                  // instead makes the black paint irrelevant, which is the whole
                  // point of using it as a stencil.
                  //
                  // The CSS `mask-image` in `MonoMark` is alpha-based already,
                  // which is why the same icon renders correctly in the author
                  // column and only the graph came out black on black.
                  <mask id={maskId} maskUnits="userSpaceOnUse" style={{ maskType: 'alpha' }}>
                    <image
                      href={avatar.url}
                      x={x - glyphR}
                      y={y - glyphR}
                      width={glyphR * 2}
                      height={glyphR * 2}
                      preserveAspectRatio="xMidYMid meet"
                    />
                  </mask>
                )}
              </defs>
              {/* A transparent PNG, or a glyph narrower than its box, lands on
                  a solid ground rather than on whatever rail runs behind. */}
              <circle
                cx={x}
                cy={y}
                r={AVATAR_OUTER_R}
                fill={avatar.bot ? '#000' : 'var(--gw-bg)'}
              />
              {avatar.mono ? (
                <rect
                  x={x - glyphR}
                  y={y - glyphR}
                  width={glyphR * 2}
                  height={glyphR * 2}
                  fill="#fff"
                  mask={`url(#${maskId})`}
                />
              ) : (
                <image
                  href={avatar.url}
                  x={x - glyphR}
                  y={y - glyphR}
                  width={glyphR * 2}
                  height={glyphR * 2}
                  clipPath={avatar.bot ? undefined : `url(#${clipId})`}
                  preserveAspectRatio={avatar.bot ? 'xMidYMid meet' : 'xMidYMid slice'}
                />
              )}
              <circle
                cx={x}
                cy={y}
                r={ringR}
                fill="none"
                stroke={sel ? 'var(--gw-text)' : col}
                strokeWidth={AVATAR_RING}
              />
            </g>
          )
        }

        return (
          <circle
            key={c.sha}
            cx={x}
            cy={y}
            r={sel ? 7.5 : overflow ? 3.5 : 6}
            fill={c.is_merge || overflow ? 'var(--gw-bg)' : col}
            stroke={sel ? 'var(--gw-text)' : c.is_merge || overflow ? col : 'var(--gw-bg)'}
            strokeWidth={sel ? 2.5 : c.is_merge ? 2.5 : overflow ? 1.5 : 2}
            opacity={overflow && !sel ? 0.7 : 1}
          />
        )
      })}
    </svg>
  )
}
