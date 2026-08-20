import { create } from 'zustand'
import type { DockKind } from '@/lib/agentWorkspaceLayout'
import type { DockZone } from '@/lib/agentDeskDockPlacement'

/**
 * Tracks the detail panel (Source/Context/Graph) currently being dragged to
 * a new dock zone, following `useDragStore`'s idiom for the ref-pill drag
 * (small store, native HTML5 DnD, one boolean the whole app dims on). This
 * is a *separate* store rather than a reuse of `useDragStore`/`useRefDnd`:
 * those are git-ref-specific (pairing rules, sync-modal dispatch) and have
 * nothing to do with docking a panel to an edge of the Agent Desk shell.
 */
interface AgentDeskDragState {
  draggingPanel: { kind: DockKind; fromZone: DockZone | 'popover' } | null
  /** The zone currently under the pointer, for live highlight during drag. */
  hoverZone: DockZone | null
  startDrag: (kind: DockKind, fromZone: DockZone | 'popover') => void
  setHoverZone: (zone: DockZone | null | ((current: DockZone | null) => DockZone | null)) => void
  endDrag: () => void
}

export const useAgentDeskDragStore = create<AgentDeskDragState>((set) => ({
  draggingPanel: null,
  hoverZone: null,
  startDrag: (kind, fromZone) => set({ draggingPanel: { kind, fromZone }, hoverZone: null }),
  setHoverZone: (zone) =>
    set((s) => ({ hoverZone: typeof zone === 'function' ? zone(s.hoverZone) : zone })),
  endDrag: () => set({ draggingPanel: null, hoverZone: null }),
}))
