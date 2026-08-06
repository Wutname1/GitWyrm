import { useEffect, useRef, useState } from 'react'
import { commands, type CommitStats } from '@/lib/bindings'

/**
 * Diff stats for the commits currently on screen.
 *
 * A log page arrives without stats for commits the backend has not already
 * computed: summarizing one means a rename-detected tree diff, and doing that
 * for a whole page costs seconds on repos with very large commits. Instead the
 * graph asks for just the rows in view, and the bar fills in a beat later.
 *
 * Requests are batched per visible range and de-duplicated across ranges, so
 * scrolling through the same commits twice only ever computes them once. The
 * backend memoizes on its side too, which makes a repeat request nearly free
 * and lets a later log refresh return the same numbers inline.
 */
export function useVisibleCommitStats(
  repoId: string | null,
  visibleShas: string[],
): Map<string, CommitStats> {
  const [stats, setStats] = useState<Map<string, CommitStats>>(new Map)
  // Shas already fetched or in flight. Kept in a ref so requesting more never
  // re-runs the effect by changing its dependencies.
  const requested = useRef(new Set<string>())

  // A new repo has its own commits and its own cache; drop everything so stats
  // from the previous repo can never show against a same-sha commit here.
  useEffect(() => {
    requested.current = new Set()
    setStats(new Map)
  }, [repoId])

  // Join the shas so the effect compares by value: the caller rebuilds this
  // array every render, and depending on its identity would refire constantly.
  const key = visibleShas.join(',')

  useEffect(() => {
    if (repoId == null) return
    const missing = visibleShas.filter((sha) => !requested.current.has(sha))
    if (missing.length === 0) return
    for (const sha of missing) requested.current.add(sha)

    let cancelled = false
    void (async () => {
      const result = await commands.getCommitStats(repoId, missing)
      if (cancelled || result.status !== 'ok') {
        // Let a failed batch be retried the next time these rows scroll by,
        // rather than leaving them permanently blank.
        if (result.status !== 'ok') {
          for (const sha of missing) requested.current.delete(sha)
        }
        return
      }
      setStats((prev) => {
        const next = new Map(prev)
        for (const [sha, s] of Object.entries(result.data)) {
          if (s) next.set(sha, s)
        }
        return next
      })
    })()

    return () => {
      cancelled = true
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps -- keyed by value, see above
  }, [repoId, key])

  return stats
}
