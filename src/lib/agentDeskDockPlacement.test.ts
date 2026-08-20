import { describe, expect, it } from 'vitest'
import {
  isNoOpDrop,
  isRightDockSafeAtWidth,
  placementToZone,
  resolveDockVisibility,
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
