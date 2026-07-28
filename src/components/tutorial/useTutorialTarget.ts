import { useEffect, useState } from 'react'

export interface TargetRect {
  top: number
  left: number
  width: number
  height: number
}

/** How much breathing room the spotlight leaves around the element, in px. */
const PADDING = 6

function measureOne(id: string): TargetRect | null {
  const node = document.querySelector<HTMLElement>(`[data-tutorial-id="${id}"]`)
  if (!node) return null
  const box = node.getBoundingClientRect()
  // A node that is present but collapsed (an unopened sidebar section, a row
  // scrolled out of a virtualized window) would otherwise punch a zero-size
  // hole in the middle of the screen.
  if (box.width === 0 || box.height === 0) return null
  return {
    top: box.top - PADDING,
    left: box.left - PADDING,
    width: box.width + PADDING * 2,
    height: box.height + PADDING * 2,
  }
}

function sameRect(a: TargetRect | null, b: TargetRect | null): boolean {
  if (a === null || b === null) return a === b
  return (
    Math.abs(a.top - b.top) < 0.5 &&
    Math.abs(a.left - b.left) < 0.5 &&
    Math.abs(a.width - b.width) < 0.5 &&
    Math.abs(a.height - b.height) < 0.5
  )
}

/**
 * Track the on-screen boxes of the elements a lesson is pointing at.
 *
 * Lessons name targets with `data-tutorial-id="..."` rather than refs, because
 * the things worth teaching live deep inside virtualized lists and context-menu
 * wrappers that we do not want to thread a ref through.
 *
 * Boxes are re-measured on scroll, resize, and DOM mutation. That last one
 * matters more than it sounds: the graph recycles rows as it scrolls, so the
 * element behind a given id can be swapped for a different node entirely while
 * the lesson is still on screen. Polling every animation frame would also work,
 * but it would run a layout read every frame for the whole lesson.
 *
 * Entries are null while their target is missing, which the overlay reads as
 * "leave that one dimmed" rather than cutting a hole in the wrong place.
 */
export function useTutorialTargets(ids: string[], enabled: boolean): (TargetRect | null)[] {
  const [rects, setRects] = useState<(TargetRect | null)[]>(() => ids.map(() => null))

  // Join the ids so the effect re-runs when the lesson changes targets, without
  // taking a new array identity as a change on every render.
  const key = ids.join('|')

  useEffect(() => {
    const list = key ? key.split('|') : []
    if (!enabled || list.length === 0) {
      setRects([])
      return
    }

    let frame = 0

    const measure = () => {
      const next = list.map(measureOne)
      // Only publish real movement: mutation observers fire constantly in an app
      // this busy, and re-setting identical rects would re-render the overlay
      // (and restart its animation) many times a second.
      setRects((prev) =>
        prev.length === next.length && prev.every((r, i) => sameRect(r, next[i])) ? prev : next
      )
    }

    // Coalesce bursts of mutations into one measurement per frame.
    const schedule = () => {
      if (frame) return
      frame = requestAnimationFrame(() => {
        frame = 0
        measure()
      })
    }

    measure()

    const observer = new MutationObserver(schedule)
    observer.observe(document.body, { childList: true, subtree: true, attributes: true })
    window.addEventListener('scroll', schedule, true)
    window.addEventListener('resize', schedule)

    return () => {
      if (frame) cancelAnimationFrame(frame)
      observer.disconnect()
      window.removeEventListener('scroll', schedule, true)
      window.removeEventListener('resize', schedule)
    }
  }, [key, enabled])

  return rects
}

/**
 * Bring a lesson's target into view before the spotlight lands on it.
 *
 * Without this, a lesson whose target is scrolled off-screen dims the whole app
 * and points at nothing, which reads as a broken tutorial rather than a hidden
 * element.
 */
export function scrollTutorialTargetIntoView(id: string) {
  const node = document.querySelector<HTMLElement>(`[data-tutorial-id="${id}"]`)
  node?.scrollIntoView({ block: 'center', inline: 'center', behavior: 'smooth' })
}
