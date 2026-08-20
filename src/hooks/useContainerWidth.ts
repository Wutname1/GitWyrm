import { useEffect, useState, type RefObject } from 'react'

/**
 * Tracks an element's own width via `ResizeObserver`.
 *
 * Container width, not viewport width: the Agent Desk conversation column
 * sits inside a window that also holds a sidebar and (sometimes) a docked
 * panel, so a viewport media query would report "wide" while the panes
 * themselves are cramped. `SessionSidebar` and `RepositoryTabs` already
 * measure their own containers the same way; this hook is that pattern
 * extracted so the workspace shell does not repeat it a third time.
 *
 * Returns `null` until the first measurement lands, so callers can treat
 * "not measured yet" as its own case rather than mistaking it for zero and
 * flashing the narrow layout on first paint.
 */
export function useContainerWidth(ref: RefObject<HTMLElement | null>): number | null {
  const [width, setWidth] = useState<number | null>(null)

  useEffect(() => {
    const node = ref.current
    if (!node) return
    const observer = new ResizeObserver((entries) => {
      const entry = entries[0]
      if (entry) setWidth(entry.contentRect.width)
    })
    observer.observe(node)
    return () => observer.disconnect()
  }, [ref])

  return width
}
