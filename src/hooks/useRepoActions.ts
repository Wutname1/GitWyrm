import { useMutation, useQuery } from '@tanstack/react-query'
import { toast } from 'sonner'
import { commands } from '@/lib/bindings'
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
      addRepo(repo)
      closeModal()
      toast.success(`Opened ${repo.name}`)
    },
    onError: (e: Error) => toast.error(e.message),
  })
}
