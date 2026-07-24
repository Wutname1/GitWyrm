import { useEffect } from 'react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { toast } from 'sonner'
import { TooltipProvider } from '@/components/ui/tooltip'
import { Toaster } from '@/components/ui/sonner'
import { WorkspaceLayout } from '@/layouts/WorkspaceLayout'
import { OnboardingModal } from '@/components/modals/OnboardingModal'
import { DirectionModal } from '@/components/modals/DirectionModal'
import { RemoteSyncModal } from '@/components/modals/RemoteSyncModal'
import { PushChoiceModal } from '@/components/modals/PushChoiceModal'
import { DragScrim } from '@/components/domain/DragScrim'
import { ErrorBoundary } from '@/components/ErrorBoundary'
import { NewBranchModal } from '@/components/modals/NewBranchModal'
import { NewTagModal } from '@/components/modals/NewTagModal'
import { PushTagsModal } from '@/components/modals/PushTagsModal'
import { RemotesModal } from '@/components/modals/RemotesModal'
import { GithubConnectModal } from '@/components/modals/GithubConnectModal'
import { useRepoWatcher } from '@/hooks/useRepoWatcher'
import { useTheme } from '@/hooks/useTheme'
import { useFont } from '@/hooks/useFont'
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

// Module-scoped, not a ref: launch restore must run exactly once per app load.
// A component ref resets whenever AppInner remounts -- which React StrictMode
// and dev hot reloads both do -- and re-running the restore would reopen tabs
// the user had deliberately closed.
let launched = false

function AppInner() {
  useRepoWatcher()
  useTheme()
  useFont()
  const openModal = useUiStore((s) => s.openModal)
  const activeRepoId = useWorkspaceStore((s) => s.activeRepoId)
  const uiScale = useWorkspaceStore((s) => s.uiScale)

  // Apply the user's zoom to the body, not #root. `zoom` scales layout and
  // every pixel value (unlike a font-size trick), which is what we want for a
  // git client full of fixed-size rows and badges.
  //
  // It has to land on the body because dialogs, popovers, dropdowns and
  // tooltips portal to document.body, which sits outside #root. Zooming #root
  // alone leaves every overlay stuck at 100% while the app behind it scales.
  //
  // Everything below is sized in percentages, not viewport units: percentages
  // resolve against the already-zoomed containing block, so the layout still
  // fits the window exactly at any scale. Viewport units (dvh/vw) do not shrink
  // under zoom, which overflows the window and pushes the status bar -- and the
  // zoom control itself -- off-screen with no way back.
  useEffect(() => {
    document.body.style.zoom = String(uiScale)
  }, [uiScale])

  // On launch: restore every previously-open tab (falling back to the most
  // recent repo, or onboarding when there is none), then re-select whichever
  // tab was active before the app closed.
  useEffect(() => {
    if (launched) return
    launched = true

    void (async () => {
      const { hydrate, addRepo, setActiveRepo, finishRepoRestore } = useWorkspaceStore.getState()
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
      finishRepoRestore()
      if (lastOpenedId) setActiveRepo(lastOpenedId)
    })()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  // Auto-enable the worktree feature the first time we see a repo that already
  // has linked worktrees, so existing worktree users get the UI without hunting
  // for the setting. Never turns it back off; that's the user's choice.
  useEffect(() => {
    if (!activeRepoId) return
    const { enableWorktrees, setEnableWorktrees } = useWorkspaceStore.getState()
    if (enableWorktrees) return
    void commands.hasWorktrees(activeRepoId).then((r) => {
      if (r.status === 'ok' && r.data) setEnableWorktrees(true)
    })
  }, [activeRepoId])

  // Suppress the browser's native right-click menu everywhere it isn't wanted.
  // Our own Radix context menus still open (they handle the event first); text
  // fields keep their native menu so copy/paste works.
  useEffect(() => {
    const onContextMenu = (e: MouseEvent) => {
      const target = e.target as HTMLElement | null
      if (target?.closest('input, textarea, [contenteditable="true"]')) return
      e.preventDefault()
    }
    document.addEventListener('contextmenu', onContextMenu)
    return () => document.removeEventListener('contextmenu', onContextMenu)
  }, [])

  return (
    <>
      <WorkspaceLayout />
      <DragScrim />
      <OnboardingModal />
      <DirectionModal />
      <RemoteSyncModal />
      <PushChoiceModal />
      <NewBranchModal />
      <NewTagModal />
      <PushTagsModal />
      <RemotesModal />
      <GithubConnectModal />
      <Toaster position="bottom-center" />
    </>
  )
}

export default function App() {
  return (
    <ErrorBoundary>
      <QueryClientProvider client={queryClient}>
        <TooltipProvider delayDuration={300}>
          <AppInner />
        </TooltipProvider>
      </QueryClientProvider>
    </ErrorBoundary>
  )
}
