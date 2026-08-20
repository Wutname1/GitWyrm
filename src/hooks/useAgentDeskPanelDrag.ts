import { type DragEvent, useCallback } from 'react'
import type { DockKind } from '@/lib/agentWorkspaceLayout'
import { ALL_DOCK_ZONES, isNoOpDrop, type DockZone } from '@/lib/agentDeskDockPlacement'
import { useAgentDeskDragStore } from '@/stores/agentDeskDragStore'

/** MIME type for the native drag payload, mirroring `REF_DND_MIME`'s pattern. */
const PANEL_DND_MIME = 'application/x-gitwyrm-agent-desk-panel'

/** Props to spread onto the element that starts a panel drag (a popover's "Pin panel" header, or a pinned dock's own header for re-docking). */
export interface PanelDragSourceProps {
  draggable: boolean
  onDragStart: (e: DragEvent) => void
  onDragEnd: () => void
}

/** Props to spread onto one drop-zone target (a placement button or an edge highlight region). */
export interface PanelDropTargetProps {
  onDragOver: (e: DragEvent) => void
  onDragLeave: () => void
  onDrop: (e: DragEvent) => void
}

/**
 * Drag wiring for moving the docked detail panel between zones (tasks.md
 * 7.5/7.6): pointer drag with a full panel ghost (the browser's default drag
 * image, sized from the dragged element, stands in for "full panel ghost"
 * here since this hook only owns the drag protocol, not a custom renderer)
 * and highlighted valid targets, with invalid/no-op drops rejected and the
 * previous placement left untouched.
 *
 * Every placement this hook can reach is also reachable through the Move
 * menu and keyboard commands (`agentDeskDockPlacement.ts`'s `ALL_DOCK_ZONES`
 * is the single list all three paths iterate) -- this hook is only the drag
 * path, not the only path.
 */
export function useAgentDeskPanelDrag(kind: DockKind, fromZone: DockZone | 'popover') {
  const startDrag = useAgentDeskDragStore((s) => s.startDrag)
  const endDrag = useAgentDeskDragStore((s) => s.endDrag)
  const setHoverZone = useAgentDeskDragStore((s) => s.setHoverZone)
  const draggingPanel = useAgentDeskDragStore((s) => s.draggingPanel)
  const hoverZone = useAgentDeskDragStore((s) => s.hoverZone)

  const dragSourceProps = useCallback(
    (): PanelDragSourceProps => ({
      draggable: true,
      onDragStart: (e) => {
        e.dataTransfer.setData(PANEL_DND_MIME, kind)
        e.dataTransfer.effectAllowed = 'move'
        startDrag(kind, fromZone)
      },
      onDragEnd: () => endDrag(),
    }),
    [kind, fromZone, startDrag, endDrag]
  )

  const dropTargetProps = useCallback(
    (zone: DockZone, onDropZone: (zone: DockZone) => void): PanelDropTargetProps => ({
      onDragOver: (e) => {
        if (!e.dataTransfer.types.includes(PANEL_DND_MIME)) return
        e.preventDefault()
        e.dataTransfer.dropEffect = 'move'
        setHoverZone(zone)
      },
      onDragLeave: () => setHoverZone((current) => (current === zone ? null : current)),
      onDrop: (e) => {
        if (!e.dataTransfer.types.includes(PANEL_DND_MIME)) return
        e.preventDefault()
        endDrag()
        if (!draggingPanel || isNoOpDrop(draggingPanel, zone)) return
        onDropZone(zone)
      },
    }),
    [draggingPanel, endDrag, setHoverZone]
  )

  return {
    dragSourceProps,
    dropTargetProps,
    dragging: draggingPanel !== null,
    draggingKind: draggingPanel?.kind ?? null,
    hoverZone,
    allZones: ALL_DOCK_ZONES,
  }
}
