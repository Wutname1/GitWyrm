import { describe, expect, it } from 'vitest'
import { edgeShape, laneGeometry, LANE_WIDTH, LANE_X0 } from './graphLanes'

describe('laneGeometry', () => {
  it('keeps a lane in the same column as the graph is resized', () => {
    // A branch must not slide sideways when the column is dragged wider, which
    // is what made the graph unreadable when spacing was derived from the
    // widest lane in the whole loaded history.
    const narrow = laneGeometry(200)
    const wide = laneGeometry(400)
    for (const lane of [0, 1, 2, 3]) {
      expect(narrow.laneX(lane)).toBe(wide.laneX(lane))
    }
  })

  it('reveals more lanes as the column widens rather than re-spacing them', () => {
    expect(laneGeometry(400).lastVisibleLane).toBeGreaterThan(
      laneGeometry(200).lastVisibleLane,
    )
  })

  it('lays lanes out at a fixed width from the left margin', () => {
    const g = laneGeometry(400)
    expect(g.laneX(0)).toBe(LANE_X0)
    expect(g.laneX(2)).toBe(LANE_X0 + 2 * LANE_WIDTH)
  })

  it('folds lanes that do not fit onto the last visible column', () => {
    const g = laneGeometry(132)
    const beyond = g.lastVisibleLane + 5
    expect(g.isOverflow(beyond)).toBe(true)
    expect(g.laneX(beyond)).toBe(g.laneX(g.lastVisibleLane))
  })

  it('spaces lanes by a caller-supplied pitch', () => {
    // Avatar nodes are wider than the default pitch and pass their own, so
    // adjacent lanes do not overlap once the nodes become pictures.
    const g = laneGeometry(400, 24)
    expect(g.laneX(0)).toBe(LANE_X0)
    expect(g.laneX(3)).toBe(LANE_X0 + 3 * 24)
  })

  it('fits fewer lanes at a wider pitch', () => {
    // The trade for a readable node: the same column folds sooner.
    expect(laneGeometry(400, 24).lastVisibleLane).toBeLessThan(
      laneGeometry(400).lastVisibleLane,
    )
  })
})

describe('edgeShape', () => {
  // The overflow column is shared by many lanes, so its x means "one of several
  // branches". Drawing a rail into it connects two commits that merely landed
  // in the same column -- the phantom line that appears to end in an unrelated
  // commit.
  const g = laneGeometry(132)
  const out = g.lastVisibleLane + 3
  const alsoOut = g.lastVisibleLane + 7

  it('draws a normal rail when both ends have their own column', () => {
    expect(edgeShape(g, 0, 1, 1)).toEqual({ kind: 'rail', track: 1 })
  })

  it('never draws a rail between two commits that only share the overflow column', () => {
    expect(g.laneX(out)).toBe(g.laneX(alsoOut))
    expect(edgeShape(g, out, alsoOut, alsoOut)).toEqual({ kind: 'hidden' })
  })

  it('stubs outward when only the parent is past the last column', () => {
    expect(edgeShape(g, 0, out, out)).toEqual({ kind: 'stub', fromLane: 0 })
  })

  it('stubs from the parent when only the commit is past the last column', () => {
    expect(edgeShape(g, out, 0, out)).toEqual({ kind: 'stub', fromLane: 0 })
  })

  it('reroutes a visible edge whose track would detour through the overflow column', () => {
    // Both endpoints are drawable, so the edge must still be drawn -- just not
    // via a track that clamps onto the shared column.
    expect(edgeShape(g, 0, 2, out)).toEqual({ kind: 'rail', track: 2 })
  })

  it('draws every edge normally when the graph is wide enough for all lanes', () => {
    const wide = laneGeometry(1000)
    expect(wide.isOverflow(out)).toBe(false)
    expect(edgeShape(wide, 0, out, out)).toEqual({ kind: 'rail', track: out })
  })
})
