import { useEffect } from 'react'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { toast } from 'sonner'
import { TooltipProvider } from '@/components/ui/tooltip'
import { Toaster } from '@/components/ui/sonner'
import { WorkspaceLayout } from '@/layouts/WorkspaceLayout'
import { OnboardingModal } from '@/components/modals/OnboardingModal'
import { TutorialHost } from '@/components/tutorial/TutorialHost'
import { TutorialOutcomeDialog } from '@/components/tutorial/TutorialOutcomeDialog'
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
import { noteRepoAvailability } from '@/hooks/useRepoActions'
import { useRepoWatcher } from '@/hooks/useRepoWatcher'
import { useTheme } from '@/hooks/useTheme'
import { useFont } from '@/hooks/useFont'
import { AUTO_CHECK_INTERVAL_MS, useUpdater } from '@/hooks/useUpdater'
import { commands, type RepoInfo } from '@/lib/bindings'
import { unwrap } from '@/lib/queryKeys'
import { log } from '@/lib/log'
import { samePath } from '@/lib/paths'
import { hideSplash, setSplashProgress, setSplashStatus } from '@/lib/splash'
import { useUiStore } from '@/stores/uiStore'
import { flushPendingSettings, useWorkspaceStore } from '@/stores/workspaceStore'

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

/**
 * Show the welcome tour only when it is still useful.
 *
 * Two conditions, either of which is enough:
 *  - the tour has never been completed, or
 *  - git still has no name and email, so the user cannot commit yet.
 *
 * The second matters for people who installed GitWyrm before the tour asked for
 * an identity: they have seen it, but never been asked, and would otherwise
 * meet the problem at their first commit instead.
 */
async function shouldShowOnboarding(seen: boolean): Promise<boolean> {
  if (!seen) return true
  const res = await commands.getGitIdentity()
  if (res.status !== 'ok') return false
  return !res.data.name.trim() || !res.data.email.trim()
}

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

    // Opens the tour only when it still has something to offer. Wraps the
    // module-level predicate so the openModal from this render is used.
    const maybeShowOnboarding = async (settings: { onboarding_seen?: boolean | null }) => {
      if (await shouldShowOnboarding(settings.onboarding_seen ?? false)) {
        openModal('onboarding')
      }
    }

    void (async () => {
      // try/finally: the splash covers the whole window, so any path out of
      // here -- including a throw -- has to lift it or the app is unreachable.
      try {
        const { hydrate, addReposInBackground, setActiveRepo, finishRepoRestore } =
          useWorkspaceStore.getState()
        const settings = await hydrate()

        // Someone who has used git for years should find profiles already
        // describing them rather than empty. Pure read of their git config --
        // it adopts what is already there without rewriting anything, and
        // no-ops once seeded or if they have made a profile themselves.
        void commands.seedProfileFromConfig().catch(() => {
          // Never worth blocking startup: profiles still work, they just
          // start empty and the Profiles screen offers to fill them in.
        })

        // Update while the splash is still up, now that the channel and
        // auto-update settings are loaded. Awaited on purpose: installing
        // before any repository work starts means nothing is half-open when
        // the app relaunches. With auto-update off this only checks, and the
        // status-bar button offers the install instead.
        await useUpdater
          .getState()
          .runLaunchUpdate(settings.auto_update !== false, setSplashStatus)

        // A folder from Explorer's right-click entry. Drained here rather than
        // delivered as an event because the backend parses it before the webview
        // exists, so an event would have had nobody listening.
        const launchPath = await commands.launchRepoPath()

        // Behavior > On startup. Off means start empty: no tabs are reopened, but
        // the saved list is left alone so turning it back on restores them.
        // A folder from the right-click menu still opens -- the user asked for
        // that one explicitly, which outranks the "start empty" preference.
        if (settings.restore_tabs === false && !launchPath) {
          await maybeShowOnboarding(settings)
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
          await maybeShowOnboarding(settings)
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
        const unavailable: string[] = []
        let lastOpenedId: string | null = null
        let launchedId: string | null = null
        results.forEach((result, i) => {
          if (result.status === 'fulfilled') {
            opened.push(result.value.repo)
            // samePath, not ===: the saved path came from a previous session and
            // can differ in case or slash style from what openRepo hands back,
            // which would silently drop the user on the wrong tab.
            if (
              settings.active_repo_path &&
              samePath(result.value.path, settings.active_repo_path)
            ) {
              lastOpenedId = result.value.repo.id
            }
            if (launchPath && samePath(result.value.path, launchPath)) {
              launchedId = result.value.repo.id
            }
          } else {
            const reason = result.reason
            const message = reason instanceof Error ? reason.message : String(reason)
            unavailable.push(toReopen[i])
            toast.error(`Failed to reopen ${toReopen[i]}: ${message}`)
          }
        })

        // Record what opened and what did not. A folder that has gone away is
        // tagged so its settings can be dropped after a week, while one that
        // reappears is untagged and keeps everything.
        for (const path of opened.map((repo) => repo.path)) {
          void noteRepoAvailability(path, true)
        }
        for (const path of unavailable) {
          void noteRepoAvailability(path, false)
        }

        // addReposInBackground, not addRepo: addRepo focuses each repo as it
        // arrives, which with parallel opens would hand the active tab to
        // whichever one happened to finish last.
        addReposInBackground(opened)
        finishRepoRestore()
        // The folder the user right-clicked wins over the tab that happened to
        // be active last session -- they just asked for this one by name.
        //
        // Last fallback: the repo that was active is gone (moved, deleted, or it
        // failed to open), so land on the first tab that did come back. toReopen
        // is in saved tab order, so that is the leftmost surviving tab rather
        // than whichever repo happened to finish opening first.
        if (settings.active_repo_path && !lastOpenedId) {
          log.warn(
            `Last active repo did not reopen: ${settings.active_repo_path}`,
          )
        }
        const firstSurvivingId = toReopen.reduce<string | null>(
          (found, path) =>
            found ??
            opened.find((repo) => samePath(repo.path, path))?.id ??
            null,
          null,
        )
        const focusId = launchedId ?? lastOpenedId ?? firstSurvivingId
        if (focusId) setActiveRepo(focusId)
      } finally {
        hideSplash()
      }
    })()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  // Settings are written on a short debounce, so switching tabs and quitting
  // right away would lose the change and reopen the tab before last. Flush
  // whatever is pending as the window closes. Covers both the title bar's close
  // button and the OS one, since both raise this event.
  //
  // Registering this listener hands the whole close to JS: Tauri stops closing
  // the window itself and the API wrapper finishes the job by calling
  // `destroy()` after the handler runs (window.js, onCloseRequested). That
  // makes `core:window:allow-destroy` load-bearing for closing the app at all.
  // It was once removed for looking unused -- no code of ours calls destroy --
  // and every close then died as an ACL denial with the window staying open.
  useEffect(() => {
    const pending = getCurrentWindow().onCloseRequested(() => {
      void flushPendingSettings()
    })
    return () => {
      void pending.then((unlisten) => unlisten())
    }
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
      {/* Above the workspace but below the modals: a lesson dims the app, and
          any dialog the lesson opens (the sync panel) must still sit on top. */}
      <TutorialHost />
      <TutorialOutcomeDialog />
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
