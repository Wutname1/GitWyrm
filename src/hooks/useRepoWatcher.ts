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
      queryClient.invalidateQueries({ queryKey: keys.log(repoId) })
      queryClient.invalidateQueries({ queryKey: keys.branches(repoId) })
      queryClient.invalidateQueries({ queryKey: keys.stashes(repoId) })
      queryClient.invalidateQueries({ queryKey: keys.tags(repoId) })
    })
    return () => {
      unlisten.then((fn) => fn())
    }
  }, [queryClient])
}
