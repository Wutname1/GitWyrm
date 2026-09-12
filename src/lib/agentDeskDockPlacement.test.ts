import { describe, expect, it } from 'vitest'
import {
  isNoOpDrop,
  isRightDockSafeAtWidth,
  placementToZone,
  resolveDockVisibility,
  isCompactWidth,
  isNarrowWidth,
  sidebarWidthAtWidth,
  zoneToPlacement,
} from './agentDeskDockPlacement'

describe('zoneToPlacement / placementToZone', () => {
  it('round-trips every zone', () => {
    const zones: Array<Parameters<typeof zoneToPlacement>[0]> = ['right', 'bottom', 'left-above', 'left-below']
    for (const zone of zones) {
      const placement = zoneToPlacement(zone)
      expect(placementToZone(placement.edge, placement.leftOrder)).toBe(zone)
    }
  })

  it('defaults left placement with no leftOrder to above-chats', () => {
    expect(placementToZone('left')).toBe('left-above')
  })
})

describe('isNoOpDrop', () => {
  it('is true when dropping back on the origin zone', () => {
    expect(isNoOpDrop({ kind: 'context', fromZone: 'right' }, 'right')).toBe(true)
  })

  it('is false when dropping on a different zone', () => {
    expect(isNoOpDrop({ kind: 'context', fromZone: 'right' }, 'bottom')).toBe(false)
  })

  it('is false when the drag originated from the popover', () => {
    expect(isNoOpDrop({ kind: 'context', fromZone: 'popover' }, 'right')).toBe(false)
  })
})

describe('isRightDockSafeAtWidth / resolveDockVisibility', () => {
  it('is unsafe below the threshold and safe at/above it', () => {
    expect(isRightDockSafeAtWidth(899)).toBe(false)
    expect(isRightDockSafeAtWidth(900)).toBe(true)
  })

  it('resolves none when no dock is open', () => {
    expect(resolveDockVisibility(null, 1200)).toBe('none')
  })

  it('resolves popover-fallback for a right dock at an unsafe width', () => {
    expect(resolveDockVisibility({ edge: 'right' }, 700)).toBe('popover-fallback')
  })

  it('resolves pinned for a right dock at a safe width', () => {
    expect(resolveDockVisibility({ edge: 'right' }, 1200)).toBe('pinned')
  })

  it('never falls back for bottom or left docks regardless of width', () => {
    expect(resolveDockVisibility({ edge: 'bottom' }, 300)).toBe('pinned')
    expect(resolveDockVisibility({ edge: 'left' }, 300)).toBe('pinned')
  })
})

/**
 * The window's own minimum is 720x560 (`spec_desk.rs`), and nothing in the
 * layout respected it: at that size the section tabs collided with New chat,
 * the workspace note truncated mid-sentence, and the composer pushed Send off
 * its edge. These pin the mockup's own breakpoints -- 900/760/620, real media
 * queries in `agent-desk-mockup.html` -- of which only 900 had been built.
 */
describe('small-window breakpoints', () => {
  const MIN_WINDOW_WIDTH = 720

  it('treats the window minimum as compact, which is what went unhandled', () => {
    expect(isCompactWidth(MIN_WINDOW_WIDTH)).toBe(true)
    // Not narrow: the chat list still fits beside the conversation at 720,
    // and the drawer breakpoint sits below the smallest allowed window.
    expect(isNarrowWidth(MIN_WINDOW_WIDTH)).toBe(false)
  })

  it('leaves a comfortable window alone', () => {
    expect(isCompactWidth(1200)).toBe(false)
    expect(isNarrowWidth(1200)).toBe(false)
  })

  it('is exclusive at the boundary, so 760 is not yet compact', () => {
    expect(isCompactWidth(760)).toBe(false)
    expect(isCompactWidth(759)).toBe(true)
    expect(isNarrowWidth(620)).toBe(false)
    expect(isNarrowWidth(619)).toBe(true)
  })

  it('narrows the chat list at the window minimum rather than dropping it', () => {
    // 176 is the mockup's 900 rule. The list stays reachable; it is only the
    // 218-282 comfortable width that does not apply here.
    expect(sidebarWidthAtWidth(MIN_WINDOW_WIDTH)).toBe(176)
  })

  it('keeps the full width once there is room', () => {
    expect(sidebarWidthAtWidth(1200)).toBe(240)
  })

  it('answers null below the drawer breakpoint, so the caller must replace it', () => {
    // Never zero: a zero-width column would render as a hairline rather than
    // telling the caller to show the drawer instead.
    expect(sidebarWidthAtWidth(600)).toBeNull()
  })
})
