import { useEffect } from 'react'
import { listen } from '@tauri-apps/api/event'
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
import { AUTO_CHECK_INTERVAL_MS, useUpdater } from '@/hooks/useUpdater'
import { commands, type RepoInfo } from '@/lib/bindings'
import { unwrap } from '@/lib/queryKeys'
import { samePath } from '@/lib/paths'
import { hideSplash, setSplashProgress } from '@/lib/splash'
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
      // try/finally: the splash covers the whole window, so any path out of
      // here -- including a throw -- has to lift it or the app is unreachable.
      try {
        const { hydrate, addReposInBackground, setActiveRepo, finishRepoRestore } =
          useWorkspaceStore.getState()
        const settings = await hydrate()

        // Look for a newer release now that the channel setting is loaded. Silent:
        // a successful "up to date" says nothing; an available update surfaces as
        // the Update button in the status bar.
        void useUpdater.getState().check(true)

        // A folder from Explorer's right-click entry. Drained here rather than
        // delivered as an event because the backend parses it before the webview
        // exists, so an event would have had nobody listening.
        const launchPath = await commands.launchRepoPath()

        // Behavior > On startup. Off means start empty: no tabs are reopened, but
        // the saved list is left alone so turning it back on restores them.
        // A folder from the right-click menu still opens -- the user asked for
        // that one explicitly, which outranks the "start empty" preference.
        if (settings.restore_tabs === false && !launchPath) {
          openModal('onboarding')
          return
        }

        // With "reopen my last tabs" off we restore nothing; only the folder the
        // user right-clicked (if any) opens.
        let saved: string[] = []
        if (settings.restore_tabs !== false) {
          const openReposList = settings.open_repos ?? []
          const recents = settings.recents ?? []
          saved =
            openReposList.length > 0
              ? openReposList
              : recents.length > 0
                ? [recents[0].path]
                : []
        }

        // The launched folder goes last so it ends up the focused tab below.
        // Filtered first so a folder that is already in the saved set opens once.
        const toReopen = launchPath
          ? [...saved.filter((path) => !samePath(path, launchPath)), launchPath]
          : saved

        if (toReopen.length === 0) {
          openModal('onboarding')
          return
        }

        // Open every tab at once rather than one after another: each openRepo is
        // an independent IPC round trip, so a serial loop made startup cost the
        // sum of all of them. allSettled keeps its results in input order, so the
        // tabs still land in the saved order no matter who answers first.
        let done = 0
        setSplashProgress(0, toReopen.length)
        const results = await Promise.allSettled(
          toReopen.map(async (path) => {
            try {
              return { path, repo: unwrap(await commands.openRepo(path)) }
            } finally {
              // Count settled, not succeeded: the bar has to reach the end even
              // when a repo has been moved or deleted since last launch.
              done += 1
              setSplashProgress(done, toReopen.length)
            }
          }),
        )

        const opened: RepoInfo[] = []
        let lastOpenedId: string | null = null
        let launchedId: string | null = null
        results.forEach((result, i) => {
          if (result.status === 'fulfilled') {
            opened.push(result.value.repo)
            if (result.value.path === settings.active_repo_path) lastOpenedId = result.value.repo.id
            if (launchPath && samePath(result.value.path, launchPath)) {
              launchedId = result.value.repo.id
            }
          } else {
            const reason = result.reason
            const message = reason instanceof Error ? reason.message : String(reason)
            toast.error(`Failed to reopen ${toReopen[i]}: ${message}`)
          }
        })

        // addReposInBackground, not addRepo: addRepo focuses each repo as it
        // arrives, which with parallel opens would hand the active tab to
        // whichever one happened to finish last.
        addReposInBackground(opened)
        finishRepoRestore()
        // The folder the user right-clicked wins over the tab that happened to
        // be active last session -- they just asked for this one by name.
        const focusId = launchedId ?? lastOpenedId
        if (focusId) setActiveRepo(focusId)
      } finally {
        hideSplash()
      }
    })()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  // Right-clicking a folder while GitWyrm is already running starts a second
  // process, which the single-instance plugin turns into this event before
  // exiting. Opens the folder as a tab in the window the user already has.
  useEffect(() => {
    const unlisten = listen<string>('open-repo-path', (event) => {
      const path = event.payload
      const { openRepos, addRepo, setActiveRepo } = useWorkspaceStore.getState()

      // Already open: focus that tab rather than opening a duplicate.
      const existing = openRepos.find((repo) => samePath(repo.path, path))
      if (existing) {
        setActiveRepo(existing.id)
        return
      }

      void (async () => {
        try {
          // addRepo, not addReposInBackground: this is a direct user action, so
          // the new tab should take focus.
          addRepo(unwrap(await commands.openRepo(path)))
        } catch (error) {
          const message = error instanceof Error ? error.message : String(error)
          toast.error(`Could not open ${path}: ${message}`)
        }
      })()
    })
    return () => {
      unlisten.then((stop) => stop())
    }
  }, [])

  // Keep looking for a newer release while the app stays open, so long-running
  // sessions still get the Update button without a restart. The launch check
  // above covers app start; this covers the hours after.
  useEffect(() => useUpdater.getState().startAutoCheck(AUTO_CHECK_INTERVAL_MS), [])

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

  // Ctrl/Cmd+F focuses the toolbar's commit search box. The toolbar owns the
  // input, so we bump a nonce it watches rather than reaching for the DOM here.
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && (e.key === 'f' || e.key === 'F')) {
        e.preventDefault()
        useUiStore.getState().requestSearchFocus()
      }
    }
    document.addEventListener('keydown', onKeyDown)
    return () => document.removeEventListener('keydown', onKeyDown)
  }, [])

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
