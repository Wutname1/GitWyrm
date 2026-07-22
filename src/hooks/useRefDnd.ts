import { type DragEvent, type PointerEvent, useCallback } from 'react'
import type { BranchList } from '@/lib/bindings'
import { REF_DND_MIME, resolveDropPair, type DraggedRef } from '@/lib/refSync'
import { useBranches } from '@/hooks/useGitQueries'
import { useDragStore } from '@/stores/dragStore'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo } from '@/stores/workspaceStore'

/** Props to spread onto a ref element to make it a drag source and drop target. */
export interface RefDndProps {
  draggable: boolean
  onPointerDown: (e: PointerEvent) => void
  onDragStart: (e: DragEvent) => void
  onDragEnd: () => void
  onDragOver: (e: DragEvent) => void
  onDrop: (e: DragEvent) => void
}

export interface RefDnd {
  props: RefDndProps
  /** True while any ref is being dragged (not necessarily this one). */
  dragging: boolean
  /** True when this element is the one being dragged. */
  isSource: boolean
  /** True when this element would accept the ref currently being dragged. */
  isValidTarget: boolean
}

/**
 * Shared drag-and-drop wiring for a ref (a branch chip in the graph or a branch
 * row in the sidebar). Dragging one ref onto another opens the sync modal for
 * whatever the two can do together. Extracted so every surface behaves the same
 * -- the same pairing rules, the same ghost, the same highlight.
 *
 * The HTML5 payload is unreadable during `dragover`, so the live highlight and
 * accept/reject decision read the drag store; the drop handler re-validates
 * against the real payload in case the store lagged on the first event.
 *
 * `buildGhost` lets a caller supply a nicer drag image (the graph clones the
 * pill); when omitted the browser's default drag image is used.
 */
export function useRefDnd(
  self: DraggedRef,
  buildGhost?: (e: DragEvent) => void
): RefDnd {
  const repo = useActiveRepo()
  const branches = useBranches(repo?.id ?? null)
  const openRemoteSync = useUiStore((s) => s.openRemoteSync)
  const draggingRef = useDragStore((s) => s.draggingRef)
  const startDrag = useDragStore((s) => s.startDrag)
  const endDrag = useDragStore((s) => s.endDrag)

  // Tags can't sync; everything else can be dragged.
  const draggable = self.type !== 'tag'

  const accepts = useCallback(
    (dragged: DraggedRef | null, data?: BranchList) => {
      const list = data ?? branches.data
      return (
        !!dragged &&
        dragged.name !== self.name &&
        !!list &&
        !!resolveDropPair(dragged, self, list)
      )
    },
    [branches.data, self]
  )

  const isSource = draggingRef?.name === self.name
  const isValidTarget = accepts(draggingRef)
  const dragging = !!draggingRef

  const onPointerDown = useCallback(
    (e: PointerEvent) => {
      // Radix's context-menu triggers wrap these elements and their pointerdown
      // handling cancels a native drag before it starts. Stop the left-button
      // press from reaching them (right-click still bubbles, so the context menu
      // keeps working).
      if (draggable && e.button === 0) e.stopPropagation()
    },
    [draggable]
  )

  const onDragStart = useCallback(
    (e: DragEvent) => {
      e.dataTransfer.setData(REF_DND_MIME, JSON.stringify(self))
      e.dataTransfer.effectAllowed = 'move'
      buildGhost?.(e)
      startDrag(self)
    },
    [self, buildGhost, startDrag]
  )

  const onDragEnd = useCallback(() => endDrag(), [endDrag])

  const onDragOver = useCallback(
    (e: DragEvent) => {
      // Only our ref drags carry this MIME type. The payload is unreadable here,
      // so accept based on the store's draggingRef; if the store hasn't
      // propagated yet (first event of a drag), accept optimistically so the
      // cursor shows "move" - the drop handler re-validates against the payload.
      if (!e.dataTransfer.types.includes(REF_DND_MIME)) return
      if (draggingRef && !isValidTarget) return
      e.preventDefault()
      e.dataTransfer.dropEffect = 'move'
    },
    [draggingRef, isValidTarget]
  )

  const onDrop = useCallback(
    (e: DragEvent) => {
      // The payload is readable here; validate against it directly so the drop
      // is correct even if the store lagged during dragover.
      const raw = e.dataTransfer.getData(REF_DND_MIME)
      const dragged = raw ? (JSON.parse(raw) as DraggedRef) : draggingRef
      if (!accepts(dragged) || !dragged) {
        endDrag()
        return
      }
      e.preventDefault()
      openRemoteSync(dragged.name, self.name)
      endDrag()
    },
    [draggingRef, accepts, endDrag, openRemoteSync, self.name]
  )

  return {
    props: { draggable, onPointerDown, onDragStart, onDragEnd, onDragOver, onDrop },
    dragging,
    isSource,
    isValidTarget,
  }
}
