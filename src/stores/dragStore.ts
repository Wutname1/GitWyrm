import { create } from 'zustand'
import type { DraggedRef } from '@/lib/refSync'

/**
 * Tracks the ref pill currently being dragged so every other pill can decide,
 * during the drag, whether it is a valid drop target and highlight itself.
 * The HTML5 dataTransfer payload is unreadable during `dragover`, so this store
 * carries the identity the target pills need to validate the pairing live.
 */
interface DragState {
  draggingRef: DraggedRef | null
  startDrag: (ref: DraggedRef) => void
  endDrag: () => void
}

export const useDragStore = create<DragState>((set) => ({
  draggingRef: null,
  startDrag: (ref) => set({ draggingRef: ref }),
  endDrag: () => set({ draggingRef: null }),
}))
