import { useEffect } from 'react'
import { useDragStore } from '@/stores/dragStore'

/** Distance from a container edge, in px, where auto-scroll kicks in. */
const EDGE = 48
/** Max pixels scrolled per animation frame at the very edge. */
const MAX_SPEED = 18

/** The nearest vertically scrollable ancestor of `el`, or null. */
function scrollableAncestor(el: Element | null): HTMLElement | null {
  let node = el as HTMLElement | null
  while (node && node !== document.body) {
    const style = getComputedStyle(node)
    const canScrollY = /(auto|scroll|overlay)/.test(style.overflowY)
    if (canScrollY && node.scrollHeight > node.clientHeight + 1) return node
    node = node.parentElement
  }
  return null
}

/**
 * Toggles `wyrm-dragging` on <body> while a ref pill is being dragged. CSS then
 * dims every region marked data-dim-on-drag. We dim the content itself rather
 * than laying a scrim over it: the virtualized graph rows form their own
 * stacking contexts, so an overlay always covered the drop targets no matter
 * their z-index.
 *
 * Also drives edge auto-scroll during a drag. A native HTML5 drag swallows the
 * mouse wheel, so a drop target scrolled off-screen would be unreachable. While
 * dragging, holding the cursor near the top or bottom of a scrollable panel
 * (the branch sidebar, the commit graph) scrolls it so you can reach the target.
 */
export function DragScrim() {
  const dragging = useDragStore((s) => s.draggingRef !== null)

  useEffect(() => {
    document.body.classList.toggle('wyrm-dragging', dragging)
    if (!dragging) return

    let raf = 0
    // The container to scroll and how fast, updated on each dragover and read by
    // the rAF loop. Kept in a ref-like closure so the loop runs at frame rate
    // rather than at the (throttled, irregular) dragover cadence.
    let target: HTMLElement | null = null
    let speed = 0

    const step = () => {
      if (target && speed !== 0) target.scrollTop += speed
      raf = requestAnimationFrame(step)
    }
    raf = requestAnimationFrame(step)

    const onDragOver = (e: DragEvent) => {
      const container = scrollableAncestor(document.elementFromPoint(e.clientX, e.clientY))
      if (!container) {
        target = null
        speed = 0
        return
      }
      const rect = container.getBoundingClientRect()
      const fromTop = e.clientY - rect.top
      const fromBottom = rect.bottom - e.clientY
      if (fromTop < EDGE) {
        target = container
        speed = -Math.round(MAX_SPEED * (1 - fromTop / EDGE))
      } else if (fromBottom < EDGE) {
        target = container
        speed = Math.round(MAX_SPEED * (1 - fromBottom / EDGE))
      } else {
        target = null
        speed = 0
      }
    }

    // Passive: we never preventDefault here; the target pills' own dragover
    // handlers decide droppability.
    document.addEventListener('dragover', onDragOver, { passive: true })
    return () => {
      document.removeEventListener('dragover', onDragOver)
      cancelAnimationFrame(raf)
    }
  }, [dragging])

  useEffect(() => () => document.body.classList.remove('wyrm-dragging'), [])

  return null
}
