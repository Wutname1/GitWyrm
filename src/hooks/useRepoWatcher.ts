import { useEffect } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import { listen } from '@tauri-apps/api/event'
import {
  invalidateOpenspec,
  isGitOperationInFlight,
  keys,
  trimLogToFirstPage,
} from '@/lib/queryKeys'

interface RepoChangedPayload {
  repo_id: string
}

/**
 * Invalidates git queries when the backend watcher reports external changes.
 *
 * `onlyRepoId` limits the refresh to one repository. The Spec Desk passes its
 * own: it has a single repo on screen, and reacting to all fourteen open tabs
 * would refetch state it never shows -- including the openspec queries, which is
 * how a single file save turned into a burst of work.
 */
export function useRepoWatcher(onlyRepoId?: string | null) {
  const queryClient = useQueryClient()

  useEffect(() => {
    const unlisten = listen<RepoChangedPayload>('repo-changed', (event) => {
      const repoId = event.payload.repo_id
      if (onlyRepoId != null && repoId !== onlyRepoId) return
      queryClient.invalidateQueries({ queryKey: keys.status(repoId) })
      // The tab badge has its own cheap query, so it refreshes here too -- an
      // edit made outside the app has to move the tab's numbers exactly like
      // one made inside it.
      queryClient.invalidateQueries({ queryKey: keys.repoCounts(repoId) })
      // Cached diffs are keyed by path and staged/unstaged only, so a rewind or
      // branch switch made outside the app leaves them pointing at the wrong
      // content under an unchanged key. Drop them alongside status.
      queryClient.invalidateQueries({ queryKey: keys.fileDiffAll(repoId) })
      // Skipped while one of our own operations is mid-flight: the writes
      // waking this watcher are that operation's intermediate state, and
      // caching it would leave a banner describing a step already finished. The
      // mutation invalidates once it settles, so the real outcome still lands.
      //
      // The ref-derived queries are in here for cost, not just correctness. A
      // push or a branch delete writes refs over several seconds, and each
      // write wakes this watcher well inside that window -- so the graph was
      // reloading two or three times per operation, each reload re-walking
      // history from the start. Waiting for the mutation to settle makes that
      // one reload of the finished state.
      if (!isGitOperationInFlight(repoId)) {
        trimLogToFirstPage(queryClient, repoId)
        queryClient.invalidateQueries({ queryKey: keys.log(repoId) })
        queryClient.invalidateQueries({ queryKey: keys.branches(repoId) })
        queryClient.invalidateQueries({ queryKey: keys.stashes(repoId) })
        queryClient.invalidateQueries({ queryKey: keys.tags(repoId) })
        queryClient.invalidateQueries({ queryKey: keys.mergeState(repoId) })
      }
      // A worktree added or removed in a terminal lands in `.git/worktrees`,
      // which the backend watcher now reports. Invalidating here is what makes
      // the section appear (or a row disappear) while the window is open and
      // focused -- re-checking when a repository becomes active cannot see a
      // terminal running beside it.
      queryClient.invalidateQueries({ queryKey: keys.worktrees(repoId) })
      // Prefix match: refreshes every open conflict file for this repo.
      queryClient.invalidateQueries({ queryKey: ['conflict', repoId] })
      // The openspec folder lives in the working tree, so the same watcher
      // covers it. An agent or editor ticking a task in tasks.md has to move
      // our progress counts exactly like our own click does.
      invalidateOpenspec(queryClient, repoId)
    })
    return () => {
      unlisten.then((fn) => fn())
    }
  }, [queryClient, onlyRepoId])
}
