import { useEffect } from 'react'
import { useDragStore } from '@/stores/dragStore'

/**
 * Toggles `wyrm-dragging` on <body> while a ref pill is being dragged. CSS then
 * dims every region marked data-dim-on-drag. We dim the content itself rather
 * than laying a scrim over it: the virtualized graph rows form their own
 * stacking contexts, so an overlay always covered the drop targets no matter
 * their z-index.
 */
export function DragScrim() {
  const dragging = useDragStore((s) => s.draggingRef !== null)

  useEffect(() => {
    document.body.classList.toggle('wyrm-dragging', dragging)
    return () => document.body.classList.remove('wyrm-dragging')
  }, [dragging])

  return null
}
