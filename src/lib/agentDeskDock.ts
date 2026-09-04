import { MIN_CHAT_SIZE_PX } from '@/lib/agentWorkspaceLayout'
/**
 * Pure decision logic for the Agent Desk workspace shell: how the
 * conversation column responds to width, what a panel drop means, and which
 * session the docked panel's contents must read (tasks.md groups 5 and 7).
 *
 * Complements `agentDeskDockPlacement.ts` rather than repeating it: that
 * module owns the *vocabulary* of dock zones (`DockZone`, `zoneToPlacement`,
 * the right-edge safety width). This one owns the *decisions* the shell makes
 * with them -- labels, drop resolution with a plain-language reason, dock
 * content scoping, resize bounds, and the responsive breakpoints.
 *
 * Lives in a plain `.ts` module rather than inside the components for the
 * reason `src/views/agentDeskViewState.ts` already spells out: this project's
 * vitest setup runs `src/**\/*.test.ts` in a Node environment with no DOM, so
 * the only logic that can be covered by a fast test is logic that does not
 * need to render.
 */

import type { DockKind, DockState, PaneId } from '@/lib/agentWorkspaceLayout'
import { ALL_DOCK_ZONES, placementToZone, type DockZone } from '@/lib/agentDeskDockPlacement'

/** Plain-language name for one dock zone, used in menus, drop targets, and toasts. */
export function zoneLabel(zone: DockZone): string {
  switch (zone) {
    case 'right':
      return 'Right side'
    case 'bottom':
      return 'Below the chat'
    case 'left-above':
      return 'Left, above the chat list'
    case 'left-below':
      return 'Left, below the chat list'
  }
}

/** Plain-language name for what the panel is showing. */
export function dockKindLabel(kind: DockKind): string {
  switch (kind) {
    case 'source':
      return 'Source'
    case 'context':
      return 'Context'
    case 'usage':
      return 'Usage'
    case 'graph':
      return 'Agent graph'
  }
}

/**
 * Every panel that can be pinned, in the order menus show them.
 *
 * Exported so a menu cannot hand-list a subset and quietly drop one -- which
 * is exactly how Usage stayed unpinnable after the type already allowed it.
 */
export const ALL_DOCK_KINDS: DockKind[] = ['source', 'context', 'usage', 'graph']

/** The zone a dock currently occupies, or null when nothing is pinned. */
export function currentZone(dock: DockState | null): DockZone | null {
  if (!dock) return null
  return placementToZone(dock.edge, dock.leftOrder)
}

export type DropOutcome =
  /** Move the panel; the caller applies it and flashes the destination. */
  | { status: 'accept'; zone: DockZone }
  /** Nothing changes, but the user still gets a visible, explained response. */
  | { status: 'reject'; reason: string }

/**
 * Decides what dropping a dragged panel onto `targetZone` means (tasks.md
 * 7.5/7.6). A rejected drop is never a silent no-op: it always carries the
 * plain-language sentence the UI shows while the panel animates back to
 * where it came from.
 */
export function resolveDrop(input: {
  /** The zone under the pointer, or null when the drop landed on nothing. */
  targetZone: DockZone | null
  /** The dock as it stands right now. */
  dock: DockState | null
  /** True when the right edge is currently unavailable because the window is too narrow. */
  rightZoneUnavailable: boolean
}): DropOutcome {
  if (!input.dock) {
    return { status: 'reject', reason: 'There is no pinned panel to move.' }
  }
  if (input.targetZone === null) {
    return { status: 'reject', reason: 'Drop the panel on one of the highlighted spots.' }
  }
  if (input.targetZone === 'right' && input.rightZoneUnavailable) {
    return { status: 'reject', reason: 'The window is too narrow for a panel on the right.' }
  }
  if (currentZone(input.dock) === input.targetZone) {
    return { status: 'reject', reason: 'The panel is already there.' }
  }
  return { status: 'accept', zone: input.targetZone }
}

/**
 * Which session the dock's contents must show (tasks.md 7.2/7.9).
 *
 * Always the *active* pane's session, never a remembered one: V1 does not
 * lock a panel to an inactive chat (design.md, "Details belong to the pane
 * that opened them"), so switching the active pane refreshes the contents
 * while leaving kind/placement/size untouched.
 */
export function dockContentSessionId(input: {
  activePane: PaneId
  primarySessionId: string | null
  secondarySessionId: string | null
  split: boolean
}): string | null {
  if (!input.split) return input.primarySessionId
  return input.activePane === 'secondary' ? input.secondarySessionId : input.primarySessionId
}

/**
 * The minimum a conversation may be squeezed to before the dock has to give
 * way, doubled when Split View puts two of them side by side.
 *
 * 360 matches the mockup's own detail panel (`clamp(360px, 36vw, 460px)` in
 * `docs/agent-desk/agent-desk-mockup.html`) and the app's compact-desktop
 * density (root `DESIGN.md`, "Resizable Workbench Rule": panes "retain useful
 * minimum and maximum widths"). At 150% Windows scaling a 1080p panel reports
 * 1280 CSS px, where 360 leaves Split View workable and a larger figure would
 * not.
 *
 * A second constant elsewhere claimed to be this same minimum with a
 * different value and was read by nothing; it is gone. Do not add another --
 * two numbers for one property can never be reconciled when only one runs.
 */
export { MIN_CHAT_SIZE_PX }

/**
 * How much room the dock may take without squeezing the chat below a usable
 * width/height (tasks.md 7.8). Returns the largest size the dock may be, so
 * the caller can feed it straight to `ResizeHandle`'s `max`.
 */
export function maxDockSizeFor(containerSizePx: number, absoluteMax: number, absoluteMin: number): number {
  const room = containerSizePx - MIN_CHAT_SIZE_PX
  if (!Number.isFinite(room)) return absoluteMax
  return Math.max(absoluteMin, Math.min(absoluteMax, Math.round(room)))
}

/**
 * The three layouts the conversation column can take, mirroring the mockup's
 * 900/760/620 breakpoints (`docs/agent-desk/agent-desk-mockup.html`).
 *
 * Measured against the *container's* width, not the viewport: this is a
 * column inside a window that also holds a sidebar and possibly a dock, so a
 * viewport media query would report "wide" while the panes themselves are
 * cramped. `SessionSidebar` already tracks its own container the same way.
 *
 *  - `wide`     (>= 760): two panes side by side.
 *  - `compact`  (620-759): split panes stack vertically so neither composer
 *                 is clipped, and button labels collapse to icons.
 *  - `narrow`   (< 620): only the active pane renders, with a pane switcher.
 */
export type ResponsiveMode = 'wide' | 'compact' | 'narrow'

export const COMPACT_BREAKPOINT_PX = 760
export const NARROW_BREAKPOINT_PX = 620

export function resolveResponsiveMode(containerWidthPx: number | null): ResponsiveMode {
  if (containerWidthPx == null) return 'wide'
  if (containerWidthPx < NARROW_BREAKPOINT_PX) return 'narrow'
  if (containerWidthPx < COMPACT_BREAKPOINT_PX) return 'compact'
  return 'wide'
}

/** True when workspace-bar buttons should show icons only (mockup's 760px rule). */
export function shouldHideButtonLabels(mode: ResponsiveMode): boolean {
  return mode !== 'wide'
}

/**
 * True when the conversation column is too narrow to keep a right-side dock
 * without making a chat unusable (tasks.md 7.10, spec "Split View remains
 * usable when space shrinks"). The dock is NOT closed when this happens --
 * the caller collapses it visually and leaves the per-pane icon button as
 * the way back to the same content.
 *
 * Complements `isRightDockSafeAtWidth` (which gates on the whole window):
 * this one also accounts for how much the dock itself is asking for and for
 * two chat minimums while split.
 */
export function shouldAutoHideRightDock(input: {
  containerWidthPx: number
  dockSizePx: number
  split: boolean
  responsiveMode: ResponsiveMode
}): boolean {
  if (input.responsiveMode !== 'wide') return true
  const chatMinimum = input.split ? MIN_CHAT_SIZE_PX * 2 : MIN_CHAT_SIZE_PX
  return input.containerWidthPx - input.dockSizePx < chatMinimum
}

export type SplitPresentation =
  /** Both panes, side by side. */
  | 'side-by-side'
  /** Both panes, stacked vertically. */
  | 'stacked'
  /** Only the active pane, with a switcher to reach the other. */
  | 'active-only'

/**
 * How the two panes are presented at the current width (tasks.md 5.8).
 * Single-pane layouts always render the one pane, whatever the width.
 */
export function resolveSplitPresentation(split: boolean, mode: ResponsiveMode): SplitPresentation {
  if (!split) return 'active-only'
  if (mode === 'narrow') return 'active-only'
  if (mode === 'compact') return 'stacked'
  return 'side-by-side'
}

/** Re-exported so menus, drag targets, and keyboard commands all iterate one list. */
export { ALL_DOCK_ZONES }
