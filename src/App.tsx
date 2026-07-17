import { useEffect, useRef } from 'react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { toast } from 'sonner'
import { TooltipProvider } from '@/components/ui/tooltip'
import { Toaster } from '@/components/ui/sonner'
import { WorkspaceLayout } from '@/layouts/WorkspaceLayout'
import { RepoPickerModal } from '@/components/modals/RepoPickerModal'
import { OnboardingModal } from '@/components/modals/OnboardingModal'
import { DirectionModal } from '@/components/modals/DirectionModal'
import { useRepoWatcher } from '@/hooks/useRepoWatcher'
import { commands } from '@/lib/bindings'
import { unwrap } from '@/lib/queryKeys'
import { useUiStore } from '@/stores/uiStore'
import { useWorkspaceStore } from '@/stores/workspaceStore'

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 5_000,
      refetchOnWindowFocus: false,
      retry: 1,
    },
  },
})

function AppInner() {
  useRepoWatcher()
  const openModal = useUiStore((s) => s.openModal)
  const launched = useRef(false)

  // On launch: restore every previously-open tab (falling back to the most
  // recent repo, or onboarding when there is none), then re-select whichever
  // tab was active before the app closed.
  useEffect(() => {
    if (launched.current) return
    launched.current = true

    void (async () => {
      const { hydrate, addRepo, setActiveRepo } = useWorkspaceStore.getState()
      const settings = await hydrate()

      const openReposList = settings.open_repos ?? []
      const recents = settings.recents ?? []
      const toReopen =
        openReposList.length > 0
          ? openReposList
          : recents.length > 0
            ? [recents[0].path]
            : []

      if (toReopen.length === 0) {
        openModal('onboarding')
        return
      }

      let lastOpenedId: string | null = null
      for (const path of toReopen) {
        try {
          const repo = unwrap(await commands.openRepo(path))
          addRepo(repo)
          if (path === settings.active_repo_path) lastOpenedId = repo.id
        } catch (e) {
          toast.error(`Failed to reopen ${path}: ${(e as Error).message}`)
        }
      }
      if (lastOpenedId) setActiveRepo(lastOpenedId)
    })()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  return (
    <>
      <WorkspaceLayout />
      <RepoPickerModal />
      <OnboardingModal />
      <DirectionModal />
      <Toaster position="bottom-center" />
    </>
  )
}

export default function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <TooltipProvider delayDuration={300}>
        <AppInner />
      </TooltipProvider>
    </QueryClientProvider>
  )
}
