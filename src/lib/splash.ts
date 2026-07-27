/**
 * Controls the boot splash defined inline in index.html.
 *
 * The splash covers the window from the very first frame (before React exists)
 * and stays up through the launch restore, so the user sees one continuous
 * loading state instead of a flash of empty app while tabs stream in. App.tsx
 * reports progress; whoever finishes the restore calls `hideSplash`.
 */

function el(): HTMLElement | null {
  return document.getElementById('splash')
}

/** Replace the line under the mark, e.g. "Checking for updates". */
export function setSplashStatus(message: string) {
  const status = document.getElementById('splash-status')
  if (status) status.textContent = message
}

/** Show "Reopening 2 of 3 repositories" under the mark. */
export function setSplashProgress(done: number, total: number) {
  const noun = total === 1 ? 'repository' : 'repositories'
  setSplashStatus(`Reopening ${done} of ${total} ${noun}`)
}

/**
 * Fade the splash out and remove it. Safe to call more than once.
 *
 * The rAF pair lets React paint before the cover lifts, but it is only a
 * nicety: a window that is minimized, occluded, or in a background tab
 * throttles rAF indefinitely, and the splash covers the whole window, so
 * hanging on a frame callback would wedge the app behind it. The timer races
 * the rAF path and wins whenever frames are not being produced.
 */
export function hideSplash() {
  const splash = el()
  if (!splash || splash.dataset.hiding === 'true') return

  const begin = () => {
    // Re-check: the racing timer and the rAF path both land here.
    if (splash.dataset.hiding === 'true') return
    splash.dataset.hiding = 'true'
    splash.addEventListener('transitionend', () => splash.remove(), { once: true })
    // The transition never fires if the element is already invisible (a
    // background window, reduced motion); clear it on a timer regardless.
    setTimeout(() => splash.remove(), 500)
  }

  requestAnimationFrame(() => requestAnimationFrame(begin))
  setTimeout(begin, 120)
}

/** Remove the splash immediately, with no fade. For crash paths. */
export function killSplash() {
  el()?.remove()
}
