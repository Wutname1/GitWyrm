/**
 * Horizontal geometry for the commit graph.
 *
 * Lanes have a fixed width so a branch keeps its column as the graph is
 * resized, and widening the column reveals more lanes rather than re-spacing
 * the ones already drawn. Lanes past what fits are folded onto the last visible
 * column, which reads as "more branches out here".
 *
 * That fold makes `laneX` many-to-one, which is the subtle part: several
 * distinct lanes share the overflow column's x. Anything that draws a line
 * between two points must therefore ask [`isOverflow`] first, or it will
 * connect two commits that merely share a clamped column and have no actual
 * relationship. See [`edgeShape`].
 */

export const LANE_X0 = 14
export const LANE_WIDTH = 20

export interface LaneGeometry {
  /** Highest lane index with a column of its own; beyond this lanes fold onto it. */
  lastVisibleLane: number
  /** Pixel centre of a lane, clamped into the overflow column. */
  laneX: (lane: number) => number
  /** True when the lane has no column of its own and shares the overflow one. */
  isOverflow: (lane: number) => boolean
}

/**
 * @param width Pixel width of the graph column.
 * @param laneWidth Pitch between lanes. Defaults to [`LANE_WIDTH`]; avatar
 *   nodes are wider than that and pass their own so adjacent lanes do not
 *   overlap. A wider pitch fits fewer lanes before the overflow fold, which is
 *   the trade for a readable node.
 */
export function laneGeometry(width: number, laneWidth: number = LANE_WIDTH): LaneGeometry {
  const lastVisibleLane = Math.max(0, Math.floor((width - LANE_X0 * 2) / laneWidth))
  return {
    lastVisibleLane,
    laneX: (lane) => LANE_X0 + Math.min(lane, lastVisibleLane) * laneWidth,
    isOverflow: (lane) => lane > lastVisibleLane,
  }
}

/**
 * How an edge from a commit to one of its parents should be drawn.
 *
 * - `rail`   - both ends have real columns; draw the usual routed rail. `track`
 *              is the lane to travel down, which is the parent's own lane when
 *              the recorded track would detour through the overflow column.
 * - `stub`   - exactly one end is in the overflow column. Only the end with a
 *              real column can be drawn truthfully, so it gets a short mark
 *              reading outward instead of a line to a meaningless x.
 * - `hidden` - both ends are folded into the overflow column, so there is no
 *              honest way to draw the edge at all.
 */
export type EdgeShape =
  | { kind: 'rail'; track: number }
  | { kind: 'stub'; fromLane: number }
  | { kind: 'hidden' }

export function edgeShape(
  geometry: LaneGeometry,
  commitLane: number,
  parentLane: number,
  /** Lane recorded for the edge itself, which can differ from the parent's. */
  trackLane: number,
): EdgeShape {
  const commitOut = geometry.isOverflow(commitLane)
  const parentOut = geometry.isOverflow(parentLane)
  if (commitOut && parentOut) return { kind: 'hidden' }
  if (commitOut !== parentOut) {
    return { kind: 'stub', fromLane: commitOut ? parentLane : commitLane }
  }
  return { kind: 'rail', track: geometry.isOverflow(trackLane) ? parentLane : trackLane }
}
