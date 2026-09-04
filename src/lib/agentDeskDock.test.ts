import { describe, expect, it } from 'vitest'
import {ALL_DOCK_KINDS, ALL_DOCK_ZONES,
  COMPACT_BREAKPOINT_PX,
  MIN_CHAT_SIZE_PX,
  NARROW_BREAKPOINT_PX,
  currentZone,
  dockContentSessionId,
  dockKindLabel,
  maxDockSizeFor,
  resolveDrop,
  resolveResponsiveMode,
  resolveSplitPresentation,
  shouldAutoHideRightDock,
  shouldHideButtonLabels,
  zoneLabel,
} from './agentDeskDock'
import { MAX_DOCK_SIZE_PX, MIN_DOCK_SIZE_PX, type DockState } from './agentWorkspaceLayout'
import { zoneToPlacement } from './agentDeskDockPlacement'

const dock = (over: Partial<DockState> = {}): DockState => ({
  kind: 'context',
  edge: 'right',
  sizePx: 360,
  ...over,
})

describe('labels', () => {
  it('names every zone the UI offers in plain language', () => {
    expect(ALL_DOCK_ZONES.map(zoneLabel)).toEqual([
      'Right side',
      'Below the chat',
      'Left, above the chat list',
      'Left, below the chat list',
    ])
  })

  it('names every panel kind in plain language', () => {
    expect(dockKindLabel('source')).toBe('Source')
    expect(dockKindLabel('context')).toBe('Context')
    expect(dockKindLabel('graph')).toBe('Agent graph')
  })
})

describe('currentZone', () => {
  it('is null when nothing is pinned', () => {
    expect(currentZone(null)).toBeNull()
  })

  it('reads left order, not just the edge', () => {
    expect(currentZone(dock({ edge: 'left', leftOrder: 'below-chats' }))).toBe('left-below')
    expect(currentZone(dock({ edge: 'left', leftOrder: 'above-chats' }))).toBe('left-above')
  })

  it('treats a left dock with no order as above-chats', () => {
    expect(currentZone(dock({ edge: 'left' }))).toBe('left-above')
  })
})

describe('resolveDrop', () => {
  it('accepts a move to a different zone', () => {
    expect(resolveDrop({ targetZone: 'bottom', dock: dock(), rightZoneUnavailable: false })).toEqual({
      status: 'accept',
      zone: 'bottom',
    })
  })

  it('rejects a drop on nothing with an explanation', () => {
    const outcome = resolveDrop({ targetZone: null, dock: dock(), rightZoneUnavailable: false })
    expect(outcome.status).toBe('reject')
    if (outcome.status === 'reject') expect(outcome.reason).toMatch(/highlighted/)
  })

  it('rejects a drop back onto the current zone', () => {
    const outcome = resolveDrop({ targetZone: 'right', dock: dock({ edge: 'right' }), rightZoneUnavailable: false })
    expect(outcome.status).toBe('reject')
    if (outcome.status === 'reject') expect(outcome.reason).toMatch(/already there/)
  })

  it('distinguishes the two left zones so a drop between them is a real move', () => {
    const outcome = resolveDrop({
      targetZone: 'left-below',
      dock: dock({ edge: 'left', leftOrder: 'above-chats' }),
      rightZoneUnavailable: false,
    })
    expect(outcome).toEqual({ status: 'accept', zone: 'left-below' })
  })

  it('rejects the right zone when the window cannot hold it', () => {
    const outcome = resolveDrop({ targetZone: 'right', dock: dock({ edge: 'bottom' }), rightZoneUnavailable: true })
    expect(outcome.status).toBe('reject')
    if (outcome.status === 'reject') expect(outcome.reason).toMatch(/too narrow/)
  })

  it('rejects when nothing is pinned at all', () => {
    expect(resolveDrop({ targetZone: 'bottom', dock: null, rightZoneUnavailable: false }).status).toBe('reject')
  })

  it('never rejects silently -- every rejection carries a reason', () => {
    const cases = [
      resolveDrop({ targetZone: null, dock: dock(), rightZoneUnavailable: false }),
      resolveDrop({ targetZone: 'right', dock: dock(), rightZoneUnavailable: false }),
      resolveDrop({ targetZone: 'right', dock: dock({ edge: 'bottom' }), rightZoneUnavailable: true }),
      resolveDrop({ targetZone: 'bottom', dock: null, rightZoneUnavailable: false }),
    ]
    for (const c of cases) {
      expect(c.status).toBe('reject')
      if (c.status === 'reject') expect(c.reason.length).toBeGreaterThan(0)
    }
  })
})

describe('dockContentSessionId', () => {
  it('reads the primary session when not split', () => {
    expect(
      dockContentSessionId({ activePane: 'secondary', primarySessionId: 'a', secondarySessionId: 'b', split: false })
    ).toBe('a')
  })

  it('follows the active pane while split', () => {
    const base = { primarySessionId: 'a', secondarySessionId: 'b', split: true } as const
    expect(dockContentSessionId({ ...base, activePane: 'primary' })).toBe('a')
    expect(dockContentSessionId({ ...base, activePane: 'secondary' })).toBe('b')
  })

  it('returns null when the active pane has no session', () => {
    expect(
      dockContentSessionId({ activePane: 'secondary', primarySessionId: 'a', secondarySessionId: null, split: true })
    ).toBeNull()
  })
})

describe('maxDockSizeFor', () => {
  it('leaves the chat its minimum width', () => {
    expect(maxDockSizeFor(1200, MAX_DOCK_SIZE_PX, MIN_DOCK_SIZE_PX)).toBe(1200 - MIN_CHAT_SIZE_PX)
  })

  it('never exceeds the absolute maximum', () => {
    expect(maxDockSizeFor(4000, MAX_DOCK_SIZE_PX, MIN_DOCK_SIZE_PX)).toBe(MAX_DOCK_SIZE_PX)
  })

  it('never drops below the absolute minimum, even in a tiny window', () => {
    expect(maxDockSizeFor(300, MAX_DOCK_SIZE_PX, MIN_DOCK_SIZE_PX)).toBe(MIN_DOCK_SIZE_PX)
  })
})

describe('shouldAutoHideRightDock', () => {
  it('hides at anything narrower than wide', () => {
    expect(
      shouldAutoHideRightDock({ containerWidthPx: 700, dockSizePx: 240, split: false, responsiveMode: 'compact' })
    ).toBe(true)
    expect(
      shouldAutoHideRightDock({ containerWidthPx: 500, dockSizePx: 240, split: false, responsiveMode: 'narrow' })
    ).toBe(true)
  })

  it('keeps a right dock when a single chat still fits beside it', () => {
    expect(
      shouldAutoHideRightDock({ containerWidthPx: 1000, dockSizePx: 360, split: false, responsiveMode: 'wide' })
    ).toBe(false)
  })

  it('needs room for two chats while split', () => {
    expect(
      shouldAutoHideRightDock({ containerWidthPx: 1000, dockSizePx: 360, split: true, responsiveMode: 'wide' })
    ).toBe(true)
    expect(
      shouldAutoHideRightDock({ containerWidthPx: 1500, dockSizePx: 360, split: true, responsiveMode: 'wide' })
    ).toBe(false)
  })

  it('hides once a growing dock eats the chat minimum', () => {
    expect(
      shouldAutoHideRightDock({ containerWidthPx: 900, dockSizePx: 600, split: false, responsiveMode: 'wide' })
    ).toBe(true)
  })
})

describe('responsive mode', () => {
  it('treats an unmeasured container as wide so nothing flashes on first paint', () => {
    expect(resolveResponsiveMode(null)).toBe('wide')
  })

  it('uses the mockup breakpoints', () => {
    expect(resolveResponsiveMode(NARROW_BREAKPOINT_PX - 1)).toBe('narrow')
    expect(resolveResponsiveMode(NARROW_BREAKPOINT_PX)).toBe('compact')
    expect(resolveResponsiveMode(COMPACT_BREAKPOINT_PX - 1)).toBe('compact')
    expect(resolveResponsiveMode(COMPACT_BREAKPOINT_PX)).toBe('wide')
  })

  it('hides button labels below the wide breakpoint', () => {
    expect(shouldHideButtonLabels('wide')).toBe(false)
    expect(shouldHideButtonLabels('compact')).toBe(true)
    expect(shouldHideButtonLabels('narrow')).toBe(true)
  })
})

describe('resolveSplitPresentation', () => {
  it('shows one pane at every width when the split is closed', () => {
    expect(resolveSplitPresentation(false, 'wide')).toBe('active-only')
    expect(resolveSplitPresentation(false, 'compact')).toBe('active-only')
    expect(resolveSplitPresentation(false, 'narrow')).toBe('active-only')
  })

  it('goes side by side, then stacked, then active-only as space shrinks', () => {
    expect(resolveSplitPresentation(true, 'wide')).toBe('side-by-side')
    expect(resolveSplitPresentation(true, 'compact')).toBe('stacked')
    expect(resolveSplitPresentation(true, 'narrow')).toBe('active-only')
  })
})

describe('every placement is reachable by every route (tasks.md 7.7 / 9.4)', () => {
  // Drag, the Move menu, and the keyboard all funnel through `resolveDrop`
  // over the same `ALL_DOCK_ZONES` list, so "reachable by drag but not by
  // menu" is structurally impossible. This pins that: starting from any
  // zone, every *other* zone is an accepted move.
  it('accepts a move from each zone to every other zone', () => {
    for (const from of ALL_DOCK_ZONES) {
      const placement = zoneToPlacement(from)
      const dockAt: DockState = { kind: 'context', ...placement, sizePx: 360 }
      for (const to of ALL_DOCK_ZONES) {
        const outcome = resolveDrop({ targetZone: to, dock: dockAt, rightZoneUnavailable: false })
        if (from === to) {
          expect(outcome.status).toBe('reject')
        } else {
          expect(outcome).toEqual({ status: 'accept', zone: to })
        }
      }
    }
  })

  it('offers all four placements, so no menu entry is missing a drop target', () => {
    expect([...ALL_DOCK_ZONES].sort()).toEqual(['bottom', 'left-above', 'left-below', 'right'])
  })
})

describe('ALL_DOCK_KINDS', () => {
  // The invariant that was missing. The pin menu hand-listed three kinds, so
  // Usage stayed unpinnable after the type already allowed it -- a menu that
  // silently drops a panel nobody notices is gone.
  it('has a plain-language label for every pinnable panel', () => {
    for (const kind of ALL_DOCK_KINDS) {
      const label = dockKindLabel(kind)
      expect(label.length).toBeGreaterThan(0)
      expect(label).not.toBe(kind)
    }
  })

  it('lists each panel exactly once', () => {
    expect(new Set(ALL_DOCK_KINDS).size).toBe(ALL_DOCK_KINDS.length)
  })

  it('includes usage, the one that was missing', () => {
    expect(ALL_DOCK_KINDS).toContain('usage')
  })
})
