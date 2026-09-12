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


/**
 * 7.10: below this window width, a pinned *right* dock is not safe to keep
 * pinned (it would crush the conversation column) and must fall back to the
 * per-pane popover instead. Other edges (bottom, left) do not compete with
 * chat width the same way, so only 'right' is gated here.
 *
 * 900 is the widest of the mockup's own three breakpoints (900/760/620, all
 * real media queries in `docs/agent-desk/agent-desk-mockup.html`), not a
 * figure derived from any chat minimum. The enforced minimum is
 * `MIN_CHAT_SIZE_PX` in `agentDeskDock.ts`; a second constant here claimed to
 * be that minimum, disagreed with it, and was read by nothing -- so the two
 * numbers could never be reconciled because only one of them ever ran.
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

/**
 * The mockup's own three breakpoints, which have always been real media
 * queries in `docs/agent-desk/agent-desk-mockup.html` (900 / 760 / 620).
 *
 * Only the 900 one was ever built, as `RIGHT_DOCK_UNSAFE_WIDTH_PX` above. The
 * other two were designed and never implemented, so the window's own minimum
 * -- 720x560, set in `spec_desk.rs` where the Agent Desk window is built --
 * sat inside a band nothing handled. At that size the section tabs collided
 * with the New chat button, the workspace note truncated mid-sentence, and
 * the composer's control row pushed Send off its own edge.
 *
 * Named for what they change rather than for their pixel value, so a caller
 * reads as a decision instead of a number comparison.
 */
export const COMPACT_WIDTH_PX = 760
export const NARROW_WIDTH_PX = 620

/**
 * Whether chrome should shed its labels and optional text.
 *
 * The mockup's 760 rule: the workspace note goes, and the layout buttons keep
 * their icons but drop their words. Nothing is removed that cannot be reached
 * another way -- every control keeps its `aria-label` and its tooltip, so the
 * affordance survives even where the word does not.
 */
export function isCompactWidth(windowWidthPx: number): boolean {
  return windowWidthPx < COMPACT_WIDTH_PX
}

/**
 * Whether the window is too narrow to carry the chat list beside the
 * conversation.
 *
 * The mockup's 620 rule hides the sidebar outright. Hiding it is only
 * honest if the chats stay reachable, so callers must pair this with a way
 * to open the list -- never use it to simply drop the navigation.
 */
export function isNarrowWidth(windowWidthPx: number): boolean {
  return windowWidthPx < NARROW_WIDTH_PX
}

/**
 * How wide the chat sidebar should be at a given window width.
 *
 * 240 is the built default and 176 is the mockup's 900 rule. Returns `null`
 * when the sidebar should not be laid out beside the conversation at all, so
 * the caller has to decide what replaces it rather than rendering a zero-width
 * column.
 */
export function sidebarWidthAtWidth(windowWidthPx: number): number | null {
  if (isNarrowWidth(windowWidthPx)) return null
  if (windowWidthPx < RIGHT_DOCK_UNSAFE_WIDTH_PX) return 176
  return 240
}
