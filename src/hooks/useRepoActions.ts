import { useMutation, useQuery } from '@tanstack/react-query'
import { toast } from 'sonner'
import { commands, type RepoInfo } from '@/lib/bindings'
import { normalizePath } from '@/lib/paths'
import { unwrap } from '@/lib/queryKeys'
import { useUiStore } from '@/stores/uiStore'
import { useWorkspaceStore } from '@/stores/workspaceStore'

/** Scanned repos in the configured code folder (no repo handles taken). */
export function useCodeFolderRepos() {
  const codeFolder = useWorkspaceStore((s) => s.codeFolder)
  return useQuery({
    queryKey: ['code-folder', codeFolder ?? 'none'],
    enabled: codeFolder != null,
    staleTime: 60_000,
    queryFn: async () => unwrap(await commands.scanCodeFolder(codeFolder!)),
  })
}

/** Opens a repo with loading feedback (toast while the backend works). */
export function useOpenRepo() {
  const addRepo = useWorkspaceStore((s) => s.addRepo)
  const closeModal = useUiStore((s) => s.closeModal)
  const closeRepoPicker = useUiStore((s) => s.closeRepoPicker)

  return useMutation({
    mutationFn: async (rawPath: string) => {
      const path = normalizePath(rawPath)
      const name = path.split('\\').pop() ?? path
      const toastId = toast.loading(`Opening ${name}…`)
      try {
        return unwrap(await commands.openRepo(path))
      } finally {
        toast.dismiss(toastId)
      }
    },
    onSuccess: (repo) => {
      // The picker did its job, so its tab retires rather than sitting there
      // next to the repository it just opened.
      closeRepoPicker()
      addRepo(repo)
      closeModal()
      toast.success(`Opened ${repo.name}`)
    },
    onError: (e: Error) => toast.error(e.message),
  })
}

/**
 * Opens several repos as tabs in one go. Failures are collected rather than
 * aborting the batch, so one unreadable folder cannot block the rest.
 */
export function useOpenRepos() {
  const addReposInBackground = useWorkspaceStore((s) => s.addReposInBackground)
  const closeModal = useUiStore((s) => s.closeModal)
  const closeRepoPicker = useUiStore((s) => s.closeRepoPicker)

  return useMutation({
    mutationFn: async (rawPaths: string[]) => {
      const toastId = toast.loading(
        `Opening ${rawPaths.length} ${rawPaths.length === 1 ? 'repository' : 'repositories'}…`
      )
      try {
        const opened: RepoInfo[] = []
        const failed: string[] = []
        for (const rawPath of rawPaths) {
          const path = normalizePath(rawPath)
          try {
            opened.push(unwrap(await commands.openRepo(path)))
          } catch {
            failed.push(path.split('\\').pop() ?? path)
          }
        }
        return { opened, failed }
      } finally {
        toast.dismiss(toastId)
      }
    },
    onSuccess: ({ opened, failed }) => {
      addReposInBackground(opened)
      if (opened.length > 0) {
        closeRepoPicker()
        closeModal()
      }
      if (failed.length === 0) {
        toast.success(`Opened ${opened.length} ${opened.length === 1 ? 'repository' : 'repositories'}`)
      } else if (opened.length > 0) {
        toast.warning(`Opened ${opened.length}, but couldn't open ${failed.join(', ')}`)
      } else {
        toast.error(`Couldn't open ${failed.join(', ')}`)
      }
    },
    onError: (e: Error) => toast.error(e.message),
  })
}
