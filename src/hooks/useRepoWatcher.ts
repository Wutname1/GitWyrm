import { useEffect } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import { listen } from '@tauri-apps/api/event'
import { keys } from '@/lib/queryKeys'

interface RepoChangedPayload {
  repo_id: string
}

/** Invalidates git queries when the backend watcher reports external changes. */
export function useRepoWatcher() {
  const queryClient = useQueryClient()

  useEffect(() => {
    const unlisten = listen<RepoChangedPayload>('repo-changed', (event) => {
      const repoId = event.payload.repo_id
      queryClient.invalidateQueries({ queryKey: keys.status(repoId) })
      // Cached diffs are keyed by path and staged/unstaged only, so a rewind or
      // branch switch made outside the app leaves them pointing at the wrong
      // content under an unchanged key. Drop them alongside status.
      queryClient.invalidateQueries({ queryKey: keys.fileDiffAll(repoId) })
      queryClient.invalidateQueries({ queryKey: keys.log(repoId) })
      queryClient.invalidateQueries({ queryKey: keys.branches(repoId) })
      queryClient.invalidateQueries({ queryKey: keys.stashes(repoId) })
      queryClient.invalidateQueries({ queryKey: keys.tags(repoId) })
      queryClient.invalidateQueries({ queryKey: keys.mergeState(repoId) })
      // Prefix match: refreshes every open conflict file for this repo.
      queryClient.invalidateQueries({ queryKey: ['conflict', repoId] })
    })
    return () => {
      unlisten.then((fn) => fn())
    }
  }, [queryClient])
}
