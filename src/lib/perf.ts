import { useEffect, useRef } from 'react'
import type { Span } from '@sentry/react'
import { Sentry } from '@/lib/sentry'

/**
 * Times a user interaction from now until the browser has painted the result,
 * and records it as a Sentry span. This is the honest way to capture the
 * latency a user actually feels for work that finishes on the render side
 * (opening a menu, switching a repo) rather than in a network/IPC call: the
 * clock stops after React commits and the next frame paints, not when the state
 * setter returns.
 *
 * Two nested `requestAnimationFrame`s wait for the paint after the commit --
 * the first fires before paint, the second after. Falls back to a microtask if
 * rAF is unavailable (it always is in a Tauri webview, but keep it safe).
 *
 * No-ops cleanly when Sentry is disabled (dev builds): the span is a stub.
 */
export function measureToPaint(name: string, op = 'ui.interaction'): void {
  const start = performance.now()
  Sentry.startSpanManual({ name, op }, (span) => {
    const stop = () => {
      span.setAttribute('duration_ms', performance.now() - start)
      span.end()
    }
    if (typeof requestAnimationFrame === 'function') {
      requestAnimationFrame(() => requestAnimationFrame(stop))
    } else {
      queueMicrotask(stop)
    }
  })
}

/**
 * Records a Sentry span for the "repo became active -> graph painted" gap: the
 * commit-log load plus first paint that happens *after* open_repo returns. This
 * is the invisible tail of repo-open latency -- open_repo's own span stops at
 * the IPC boundary, and nothing else covers the render.
 *
 * Opens a span the moment a new repo id starts loading, and closes it after the
 * paint that follows the load finishing. Switching repos (new id) starts a
 * fresh measurement; an already-cached repo that never enters the loading state
 * is not measured, which is correct -- there was no felt gap to time.
 */
export function useGraphLoadSpan(repoId: string | null, isLoading: boolean): void {
  const spanRef = useRef<Span | null>(null)
  const measuredId = useRef<string | null>(null)

  useEffect(() => {
    if (repoId == null) return

    // A new repo that is loading: open a span once for this id.
    if (isLoading && measuredId.current !== repoId && spanRef.current == null) {
      measuredId.current = repoId
      Sentry.startSpanManual({ name: 'graph.load', op: 'ui.load' }, (span) => {
        span.setAttribute('repo_id', repoId)
        spanRef.current = span
        // The span is closed by the load-finished branch below, not here.
      })
      return
    }

    // The load for the span we opened just finished: stop after the next paint.
    if (!isLoading && spanRef.current != null && measuredId.current === repoId) {
      const span = spanRef.current
      spanRef.current = null
      const stop = () => span.end()
      if (typeof requestAnimationFrame === 'function') {
        requestAnimationFrame(() => requestAnimationFrame(stop))
      } else {
        queueMicrotask(stop)
      }
    }
  }, [repoId, isLoading])

  // Close a dangling span if the component unmounts mid-load.
  useEffect(() => {
    return () => {
      if (spanRef.current != null) {
        spanRef.current.end()
        spanRef.current = null
      }
    }
  }, [])
}
