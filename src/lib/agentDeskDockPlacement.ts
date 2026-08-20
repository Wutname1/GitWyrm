/**
 * Pure logic for the docked detail panel's placement (tasks.md group 7):
 * which drop zones are valid, what a drag/keyboard placement command
 * resolves to, and when the right dock must fall back to its popover instead
 * of being pinned (7.10, "unsafe widths").
 */

import type { DockEdge, DockKind, LeftDockOrder } from '@/lib/agentWorkspaceLayout'

export type DockZone = 'right' | 'bottom' | 'left-above' | 'left-below'

/** One physical drop target's edge/order pair. */
export function zoneToPlacement(zone: DockZone): { edge: DockEdge; leftOrder?: LeftDockOrder } {
  switch (zone) {
    case 'right':
      return { edge: 'right' }
    case 'bottom':
      return { edge: 'bottom' }
    case 'left-above':
      return { edge: 'left', leftOrder: 'above-chats' }
    case 'left-below':
      return { edge: 'left', leftOrder: 'below-chats' }
  }
}

export function placementToZone(edge: DockEdge, leftOrder?: LeftDockOrder): DockZone {
  if (edge === 'right') return 'right'
  if (edge === 'bottom') return 'bottom'
  return leftOrder === 'below-chats' ? 'left-below' : 'left-above'
}

/** All zones a drag/keyboard command can target, in a stable order for menus and Tab order. */
export const ALL_DOCK_ZONES: DockZone[] = ['right', 'bottom', 'left-above', 'left-below']

export interface DockDragState {
  kind: DockKind
  fromZone: DockZone | 'popover'
}

/**
 * Whether dropping on `zone` is a real move (tasks.md 7.6: reject invalid
 * drops visibly and restore the previous placement). Dropping back onto the
 * zone the panel already occupies is not an error, but it is also not a
 * move worth persisting -- callers can use this to skip a no-op write.
 */
export function isNoOpDrop(drag: DockDragState, targetZone: DockZone): boolean {
  return drag.fromZone === targetZone
}

/** Every zone is a valid drop target today; kept as a function (not a constant use-site) so a future width/kind restriction has one place to add a rule. */
export function isValidDockZone(_zone: DockZone): boolean {
  return true
}

/** design.md's minimum chat width the dock must never shrink below. */
export const MIN_CHAT_WIDTH_PX = 420

/**
 * 7.10: below this window width, a pinned *right* dock is not safe to keep
 * pinned (it would crush the chat column under `MIN_CHAT_WIDTH_PX`) and must
 * fall back to the per-pane popover instead. Other edges (bottom, left) do
 * not compete with chat width the same way, so only 'right' is gated here.
 */
export const RIGHT_DOCK_UNSAFE_WIDTH_PX = 900

export function isRightDockSafeAtWidth(windowWidthPx: number): boolean {
  return windowWidthPx >= RIGHT_DOCK_UNSAFE_WIDTH_PX
}

/**
 * Resolves whether the dock should currently render pinned or fall back to
 * "popover only" at the given window width. Only the `right` edge is width
 * sensitive per 7.10.
 */
export function resolveDockVisibility(
  dock: { edge: DockEdge } | null,
  windowWidthPx: number
): 'pinned' | 'popover-fallback' | 'none' {
  if (!dock) return 'none'
  if (dock.edge === 'right' && !isRightDockSafeAtWidth(windowWidthPx)) return 'popover-fallback'
  return 'pinned'
}
