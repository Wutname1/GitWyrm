import { useEffect, useRef } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import { listen } from '@tauri-apps/api/event'
import { Loader2 } from 'lucide-react'
import { toast } from 'sonner'
import { refreshAfterBranchUpdate } from '@/hooks/useGitMutations'
import { commands, type UpdateAllProgress, type UpdateAllReport } from '@/lib/bindings'
import { plural } from '@/lib/gitDisplay'
import { log } from '@/lib/log'
import { pathKey, pathName, samePath } from '@/lib/paths'
import { beginGitOperation } from '@/lib/queryKeys'
import { readWindowMode } from '@/lib/windowMode'
import {
  everyProjectRequest,
  showUpdateResultsToast,
  startUpdateAll,
  stopUpdateAll,
  useUpdateAllStore,
} from '@/stores/updateAllStore'
import { useWorkspaceStore } from '@/stores/workspaceStore'

const TOAST_ID = 'update-all'

/** How long after launch the on-start run waits, so it never competes with tab restore. */
const START_DELAY_MS = 5000

let startedThisSession = false

const openDetails = () => useUpdateAllStore.getState().openDialog()

/** The toast's text is the way into the details window: sonner has no click handler of its own. */
function ProgressTitle() {
  const progress = useUpdateAllStore((s) => s.progress)
  if (!progress) return null
  return (
    <button type="button" onClick={openDetails} className="cursor-pointer text-left">
      {progress.stopping
        ? 'Stopping…'
        : progress.total === 0
          ? 'Finding your projects…'
          : `Getting the latest · ${progress.done} of ${plural(progress.total, 'project')}`}
    </button>
  )
}

function ProgressDetail() {
  const progress = useUpdateAllStore((s) => s.progress)
  if (!progress) return null
  const pct = progress.total > 0 ? Math.round((progress.done / progress.total) * 100) : 0
  const counts = [
    progress.branches_updated > 0 && `${plural(progress.branches_updated, 'branch', 'branches')} updated`,
    progress.errors + progress.warnings > 0 &&
      `${plural(progress.errors + progress.warnings, 'project')} need${progress.errors + progress.warnings === 1 ? 's' : ''} a look`,
  ].filter(Boolean)
  const now = progress.running.map((r) => pathName(r.path)).join(', ')
  return (
    <span
      role="button"
      tabIndex={0}
      onClick={openDetails}
      onKeyDown={(e) => (e.key === 'Enter' || e.key === ' ') && openDetails()}
      className="mt-1 flex w-full min-w-0 cursor-pointer flex-col gap-1.5"
    >
      <span
        role="progressbar"
        aria-valuemin={0}
        aria-valuemax={progress.total}
        aria-valuenow={progress.done}
        className="block h-1 w-full overflow-hidden rounded-full bg-panel3"
      >
        <span
          className="block h-full rounded-full bg-accent-text transition-[width] duration-300"
          style={{ width: progress.total > 0 ? `${pct}%` : '8%' }}
        />
      </span>
      {progress.running.length > 0 && (
        <span className="block min-w-0 truncate text-2xs" title={now}>
          Now: {now}
        </span>
      )}
      {counts.length > 0 && <span className="text-2xs">{counts.join(' · ')}</span>}
      <span className="text-2xs text-accent-text underline underline-offset-2">Show details</span>
    </span>
  )
}

/** Raise (or re-raise, after it was closed) the live progress toast. */
export function showUpdateAllProgressToast() {
  toast(<ProgressTitle />, {
    id: TOAST_ID,
    duration: Infinity,
    icon: <Loader2 className="size-4 animate-spin" />,
    description: <ProgressDetail />,
    action: {
      label: 'Stop',
      onClick: (e) => {
        // Keep the toast up: it goes on reporting until the job winds down.
        e.preventDefault()
        void stopUpdateAll()
      },
    },
  })
}

/**
 * Follows the background "get the latest" job. Renders nothing.
 *
 * - Drives the progress toast and replaces it with a summary when done.
 * - Marks each open repository busy while the job works on it, so the file
 *   watcher does not reload the graph over and over mid-fetch, then refreshes
 *   that tab once it is finished.
 * - Starts a run a few seconds after launch when the user asked for that.
 */
export function UpdateAllSync() {
  const qc = useQueryClient()
  const hydrated = useWorkspaceStore((s) => s.hydrated)
  const updateAllOnStart = useWorkspaceStore((s) => s.updateAllOnStart)
  const busy = useRef(new Map<string, { repoId: string; end: () => void }>())

  useEffect(() => {
    const track = (running: string[]) => {
      const openRepos = useWorkspaceStore.getState().openRepos
      const runningKeys = new Set(running.map(pathKey))
      for (const path of running) {
        const key = pathKey(path)
        if (busy.current.has(key)) continue
        const open = openRepos.find((repo) => samePath(repo.path, path))
        if (open)
          busy.current.set(key, {
            repoId: open.id,
            end: beginGitOperation(open.id),
          })
      }
      for (const [key, entry] of busy.current) {
        if (runningKeys.has(key)) continue
        entry.end()
        refreshAfterBranchUpdate(qc, entry.repoId)
        busy.current.delete(key)
      }
    }

    const onProgress = (progress: UpdateAllProgress) => {
      useUpdateAllStore.getState().setProgress(progress)
      track(progress.running.map((r) => r.path))
    }

    const onFinished = (report: UpdateAllReport) => {
      track([])
      const results = useUpdateAllStore.getState().finishJob(report.repos, report.cancelled, report.finished_at * 1000)
      showUpdateResultsToast(results, TOAST_ID)
    }

    const unlisteners = [
      listen<UpdateAllProgress>('update-all-progress', (e) => onProgress(e.payload)),
      listen<UpdateAllReport>('update-all-finished', (e) => onFinished(e.payload)),
    ]

    // A reload mid-run misses the events that came before it.
    void commands.updateAllState().then((state) => {
      if (state.status !== 'ok') return
      const store = useUpdateAllStore.getState()
      if (state.data.last && !store.results) {
        store.showResults({
          repos: state.data.last.repos,
          cancelled: state.data.last.cancelled,
          finishedAt: state.data.last.finished_at * 1000,
          scope: 'all',
        })
      }
      if (state.data.running) onProgress(state.data.running)
    })

    return () => {
      for (const unlisten of unlisteners) void unlisten.then((fn) => fn())
      for (const entry of busy.current.values()) entry.end()
      busy.current.clear()
    }
  }, [qc])

  // The toast appears the moment a run starts, before the backend has said
  // anything, and goes away if the run could not start at all.
  useEffect(
    () =>
      useUpdateAllStore.subscribe((state, prev) => {
        if (state.progress && !prev.progress) showUpdateAllProgressToast()
        if (!state.progress && prev.progress?.job === 0) toast.dismiss(TOAST_ID)
      }),
    [],
  )

  useEffect(() => {
    if (!hydrated || !updateAllOnStart || startedThisSession) return
    if (readWindowMode().kind !== 'main') return
    const timer = window.setTimeout(() => {
      if (startedThisSession) return
      startedThisSession = true
      const request = everyProjectRequest()
      if (request.folders.length === 0 && request.paths.length === 0) return
      log.info('update-all: starting the on-launch run')
      void startUpdateAll(request)
    }, START_DELAY_MS)
    return () => window.clearTimeout(timer)
  }, [hydrated, updateAllOnStart])

  return null
}
