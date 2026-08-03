import { useEffect, useState } from 'react'
import { getCurrentWindow } from '@tauri-apps/api/window'

const inTauri = '__TAURI_INTERNALS__' in window

/**
 * Whether the window is currently maximized, for drawing the restore/maximize
 * glyph.
 *
 * `onResized` fires once per frame while a window is being dragged or resized,
 * and each `isMaximized()` is a full IPC round-trip. Asking on every event put
 * this among the most-called commands in the app -- hundreds of calls per
 * session -- all competing with real work for the IPC thread during the exact
 * moments the UI most needs to stay responsive.
 *
 * The answer only changes when a resize *settles*, so the query is debounced to
 * the end of the gesture. The trailing call is the one that matters; the
 * intermediate frames would all report the same value anyway.
 */
export function useIsMaximized(): boolean {
  const [isMax, setIsMax] = useState(false)

  useEffect(() => {
    if (!inTauri) return
    const win = getCurrentWindow()
    let timer: ReturnType<typeof setTimeout> | undefined
    let cancelled = false

    const check = () => {
      win
        .isMaximized()
        .then((v) => {
          if (!cancelled) setIsMax(v)
        })
        .catch(() => {})
    }

    check()
    const un = win.onResized(() => {
      if (timer) clearTimeout(timer)
      timer = setTimeout(check, 150)
    })

    return () => {
      cancelled = true
      if (timer) clearTimeout(timer)
      un.then((u) => u()).catch(() => {})
    }
  }, [])

  return isMax
}
