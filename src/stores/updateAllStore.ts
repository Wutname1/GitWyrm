import { create } from 'zustand'
import { toast } from 'sonner'
import { commands, type RepoUpdate, type UpdateAllProgress, type UpdateAllRequest } from '@/lib/bindings'
import { log } from '@/lib/log'
import { mergeRepoUpdates, sortRepoUpdates, updateDetail, updateHeadline, updateTotals } from '@/lib/updateAll'
import { useWorkspaceStore } from '@/stores/workspaceStore'

/** What the results window shows. */
export interface UpdateResults {
  repos: RepoUpdate[]
  cancelled: boolean
  /** Epoch milliseconds. */
  finishedAt: number
  /** Whether this came from one repository's Branch Manager or a run over all projects. */
  scope: 'all' | 'repo'
}

interface UpdateAllState {
  /** The running job, or null when nothing is running. */
  progress: UpdateAllProgress | null
  results: UpdateResults | null
  resultsOpen: boolean
  /**
   * Set when the running job is a retry started from the results window, so
   * its report is folded into what is already there rather than replacing it.
   */
  mergeNext: boolean
  setProgress: (progress: UpdateAllProgress | null) => void
  showResults: (results: UpdateResults, open?: boolean) => void
  /** Store a finished job's report, merging it if it was a retry. */
  finishJob: (repos: RepoUpdate[], cancelled: boolean, finishedAt: number) => UpdateResults
  openResults: () => void
  closeResults: () => void
}

export const useUpdateAllStore = create<UpdateAllState>((set, get) => ({
  progress: null,
  results: null,
  resultsOpen: false,
  mergeNext: false,
  setProgress: (progress) => set({ progress }),
  showResults: (results, open = false) =>
    set({ results: { ...results, repos: sortRepoUpdates(results.repos) }, resultsOpen: open || get().resultsOpen }),
  finishJob: (repos, cancelled, finishedAt) => {
    const { mergeNext, results } = get()
    const next: UpdateResults =
      mergeNext && results
        ? { ...results, repos: mergeRepoUpdates(results.repos, repos), cancelled, finishedAt }
        : { repos: sortRepoUpdates(repos), cancelled, finishedAt, scope: 'all' }
    set({ results: next, progress: null, mergeNext: false })
    return next
  },
  openResults: () => set({ resultsOpen: true }),
  closeResults: () => set({ resultsOpen: false }),
}))

/**
 * Start a background update. `retry` marks one started from the results
 * window, whose report is merged into the one on screen.
 */
export async function startUpdateAll(request: UpdateAllRequest, retry = false): Promise<boolean> {
  const store = useUpdateAllStore.getState()
  if (store.progress) {
    toast('Already getting the latest. It will finish shortly.')
    return false
  }
  // Show intent right away: the first event can take a moment while the code
  // folders are scanned.
  store.setProgress({
    job: 0,
    total: 0,
    done: 0,
    running: [],
    branches_updated: 0,
    commits_received: 0,
    errors: 0,
    warnings: 0,
    stopping: false,
  })
  useUpdateAllStore.setState({ mergeNext: retry })
  const result = await commands.updateAllStart(request)
  if (result.status === 'error') {
    log.warn(`update-all could not start: ${result.error}`)
    useUpdateAllStore.setState({ progress: null, mergeNext: false })
    toast.error(result.error)
    return false
  }
  return true
}

/** Every project in the code folders, plus any open tab that lives elsewhere. */
export function everyProjectRequest(): UpdateAllRequest {
  const { codeFolders, openRepos } = useWorkspaceStore.getState()
  return {
    folders: codeFolders.map((folder) => folder.path),
    paths: openRepos.map((repo) => repo.path),
    allow_set_aside: [],
    sign_in: false,
  }
}

export async function stopUpdateAll() {
  const result = await commands.updateAllCancel()
  if (result.status === 'error') toast.error(result.error)
}

/**
 * Show a finished run as a toast with a way into the results. `id` replaces
 * the progress toast for a background run.
 */
export function showUpdateResultsToast(results: UpdateResults, id: string) {
  const totals = updateTotals(results.repos)
  const headline = updateHeadline(totals)
  // One project with a problem of its own (it could not fetch) says so
  // directly: there is nothing else in the result worth reading first.
  const single = results.scope === 'repo' ? results.repos[0] : undefined
  const detail = single?.message ?? updateDetail(totals, results.cancelled, results.scope)
  const needsLook = totals.errors + totals.warnings > 0
  const show = needsLook ? toast.warning : toast.success
  show(headline, {
    id,
    icon: undefined,
    description: detail || undefined,
    duration: needsLook ? 20_000 : 8_000,
    action: {
      label: 'See results',
      onClick: () => useUpdateAllStore.getState().openResults(),
    },
  })
}

/** Show one repository's "get the latest" result, from the Branch Manager. */
export function showRepoUpdateResult(report: RepoUpdate) {
  const results: UpdateResults = { repos: [report], cancelled: false, finishedAt: Date.now(), scope: 'repo' }
  useUpdateAllStore.getState().showResults(results)
  showUpdateResultsToast(results, 'update-branches')
}
