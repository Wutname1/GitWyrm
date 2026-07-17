import { useDragStore } from '@/stores/dragStore'

/**
 * A full-window dim shown while a ref pill is being dragged. It darkens
 * everything so the pulsing drop targets (which sit above it) stand out. Purely
 * presentational and click-through (pointer-events: none) so it never blocks the
 * drag itself.
 */
export function DragScrim() {
  const dragging = useDragStore((s) => s.draggingRef !== null)
  if (!dragging) return null
  return <div className="wyrm-drag-scrim" />
}
