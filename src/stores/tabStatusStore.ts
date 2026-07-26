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
/** One repository's pending work, split the way the tab badges show it. */
export interface RepoChangeBreakdown {
  ahead: number
  behind: number
  uncommitted: number
}

interface TabStatusState {
  /** Total ahead + behind + uncommitted per repo, keyed by lowercased path. */
  changeCounts: TabChangeCounts
  /**
   * The same numbers kept apart, so a collapsed group's header can roll up its
   * members into push / pull / uncommitted totals rather than one lump sum.
   */
  changeBreakdowns: Record<string, RepoChangeBreakdown>
  /** Publish one repo's counts. Zeroes are kept so ties sort predictably. */
  reportChangeCount: (repoPath: string, counts: RepoChangeBreakdown) => void
  /**
   * Forget every repo that is no longer open. Counts deliberately survive a tab
   * unmounting: collapsing a group unmounts its member tabs, and dropping their
   * counts there would make the group's sort weight fall to zero and send it
   * jumping down the strip. Only closing a repository should forget it.
   */
  pruneChangeCounts: (openPaths: string[]) => void
}

function pathKey(path: string): string {
  return normalizePath(path).toLowerCase()
}

export const useTabStatusStore = create<TabStatusState>()((set) => ({
  changeCounts: {},
  changeBreakdowns: {},
  reportChangeCount: (repoPath, counts) => {
    const key = pathKey(repoPath)
    const total = counts.ahead + counts.behind + counts.uncommitted
    set((s) => {
      const previous = s.changeBreakdowns[key]
      if (
        s.changeCounts[key] === total &&
        previous?.ahead === counts.ahead &&
        previous.behind === counts.behind &&
        previous.uncommitted === counts.uncommitted
      )
        return s
      return {
        changeCounts: { ...s.changeCounts, [key]: total },
        changeBreakdowns: { ...s.changeBreakdowns, [key]: counts },
      }
    })
  },
  pruneChangeCounts: (openPaths) => {
    const open = new Set(openPaths.map(pathKey))
    set((s) => {
      const stale = Object.keys(s.changeCounts).filter((key) => !open.has(key))
      if (stale.length === 0) return s
      const changeCounts = { ...s.changeCounts }
      const changeBreakdowns = { ...s.changeBreakdowns }
      for (const key of stale) {
        delete changeCounts[key]
        delete changeBreakdowns[key]
      }
      return { changeCounts, changeBreakdowns }
    })
  },
}))
