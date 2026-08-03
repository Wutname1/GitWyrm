import type { QueryClient } from '@tanstack/react-query'
import type { DiffSource } from './bindings'

export const keys = {
  log: (repoId: string) => ['log', repoId] as const,
  status: (repoId: string) => ['status', repoId] as const,
  branches: (repoId: string) => ['branches', repoId] as const,
  tags: (repoId: string) => ['tags', repoId] as const,
  remoteTags: (repoId: string, remote: string) => ['remoteTags', repoId, remote] as const,
  remotes: (repoId: string) => ['remotes', repoId] as const,
  stashes: (repoId: string) => ['stashes', repoId] as const,
  submodules: (repoId: string) => ['submodules', repoId] as const,
  worktrees: (repoId: string) => ['worktrees', repoId] as const,
  commitDetail: (repoId: string, sha: string) => ['commit', repoId, sha] as const,
  fileDiff: (repoId: string, path: string, source: DiffSource) =>
    ['diff', repoId, path, source] as const,
  fileHistory: (repoId: string, path: string) => ['fileHistory', repoId, path] as const,
  fileBlame: (repoId: string, path: string, sha: string | null) =>
    ['fileBlame', repoId, path, sha] as const,
  mergeState: (repoId: string) => ['mergeState', repoId] as const,
  conflict: (repoId: string, path: string) => ['conflict', repoId, path] as const,
  openspecStatus: (repoId: string) => ['openspecStatus', repoId] as const,
  openspecChanges: (repoId: string) => ['openspecChanges', repoId] as const,
  openspecArchived: (repoId: string) => ['openspecArchived', repoId] as const,
  openspecHistory: (repoId: string, changeId: string) =>
    ['openspecHistory', repoId, changeId] as const,
  /** Prefix: every change's history for one repo. */
  openspecHistoryAll: (repoId: string) => ['openspecHistory', repoId] as const,
  specLink: (repoId: string, branch: string) => ['specLink', repoId, branch] as const,
  /** Prefix: every branch's spec link for one repo. */
  specLinkAll: (repoId: string) => ['specLink', repoId] as const,

  /**
   * Prefixes for invalidating every entry of a kind for one repo, regardless of
   * the trailing segments. Use these instead of hand-writing a shorter array:
   * a literal bypasses the factory, so it silently stops matching if a key ever
   * gains or reorders a segment.
   */
  remoteTagsAll: (repoId: string) => ['remoteTags', repoId] as const,
  fileDiffAll: (repoId: string) => ['diff', repoId] as const,
}

/**
 * Refresh everything derived from a repo's `openspec/` folder.
 *
 * Called both after our own writes and when the watcher reports an external
 * edit, because the files are the state: an agent or editor ticking a task has
 * to move the same counts our own click does.
 */
export function invalidateOpenspec(qc: QueryClient, repoId: string) {
  qc.invalidateQueries({ queryKey: keys.openspecStatus(repoId) })
  qc.invalidateQueries({ queryKey: keys.openspecChanges(repoId) })
  qc.invalidateQueries({ queryKey: keys.openspecArchived(repoId) })
  qc.invalidateQueries({ queryKey: keys.openspecHistoryAll(repoId) })
}

/**
 * Drop all but the first page of the commit log before it refetches.
 *
 * An infinite query refetches every page it is holding, one after another,
 * because each page's offset comes from the page before it. `get_log` also
 * re-walks history from the start to reach its offset, so page five costs five
 * times page one. Together that makes a refresh of a deeply scrolled graph slow
 * enough to lag visibly behind the toast and the file list -- the graph appears
 * to update late even though it was invalidated at the same moment.
 *
 * Trimming first means the refresh is always one page. The graph snaps back to
 * the top of history, which is where a rewind, commit or merge has just moved
 * things anyway, and scrolling down reloads the rest on demand.
 */
export function trimLogToFirstPage(qc: QueryClient, repoId: string) {
  qc.setQueryData<{ pages: unknown[]; pageParams: unknown[] }>(keys.log(repoId), (data) => {
    if (!data || data.pages.length <= 1) return data
    return { pages: data.pages.slice(0, 1), pageParams: data.pageParams.slice(0, 1) }
  })
}

/** Unwraps tauri-specta's Result<T, string> into T-or-throw for TanStack Query. */
export function unwrap<T>(result: { status: 'ok'; data: T } | { status: 'error'; error: string }): T {
  if (result.status === 'error') throw new Error(result.error)
  return result.data
}
