import { create } from 'zustand'
import { normalizePath } from '@/lib/paths'
import type { TabChangeCounts } from '@/lib/tabSorting'

/**
 * Live change counts per repository tab, in memory only.
 *
 * Sorting tabs by "has changes" needs every open repo's counts at once, but the
 * counts come from per-repo queries that only the individual tab components
 * subscribe to. Rather than re-running those queries at the strip level, each
 * tab reports its own total here and the strip reads the collected map.
 */
interface TabStatusState {
  /** Total ahead + behind + uncommitted per repo, keyed by lowercased path. */
  changeCounts: TabChangeCounts
  /** Publish one repo's count. A zero count is kept so ties sort predictably. */
  reportChangeCount: (repoPath: string, count: number) => void
  /** Drop a repo's entry when its tab unmounts. */
  clearChangeCount: (repoPath: string) => void
}

function pathKey(path: string): string {
  return normalizePath(path).toLowerCase()
}

export const useTabStatusStore = create<TabStatusState>()((set) => ({
  changeCounts: {},
  reportChangeCount: (repoPath, count) => {
    const key = pathKey(repoPath)
    set((s) =>
      s.changeCounts[key] === count
        ? s
        : { changeCounts: { ...s.changeCounts, [key]: count } },
    )
  },
  clearChangeCount: (repoPath) => {
    const key = pathKey(repoPath)
    set((s) => {
      if (!(key in s.changeCounts)) return s
      const changeCounts = { ...s.changeCounts }
      delete changeCounts[key]
      return { changeCounts }
    })
  },
}))
