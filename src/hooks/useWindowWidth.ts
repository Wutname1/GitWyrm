import { useEffect, useState } from 'react'

/**
 * The window's current inner width, for layout decisions that depend on it.
 *
 * `AgentDeskView` tracked this inline and was the only place that knew it, so
 * the chrome around it -- the titlebar, the workspace toolbar, the composer --
 * had no way to react and simply overflowed at the window's own 720px
 * minimum. Shared here rather than passed down, because the components that
 * need it are not in one subtree.
 *
 * Reads `window.innerWidth` on mount rather than assuming a size: Agent Desk
 * opens at a restored position, so the first paint can already be narrow.
 */
export function useWindowWidth(): number {
  const [width, setWidth] = useState(() => window.innerWidth)
  useEffect(() => {
    const onResize = () => setWidth(window.innerWidth)
    window.addEventListener('resize', onResize)
    // A resize can land between the initial state and this effect running.
    onResize()
    return () => window.removeEventListener('resize', onResize)
  }, [])
  return width
}
