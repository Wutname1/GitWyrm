import type { QueryClient } from '@tanstack/react-query'
import type { DiffSource } from './bindings'

export const keys = {
  log: (repoId: string, hiddenBranches: string[] = [], focusedBranch: string | null = null) =>
    ['log', repoId, hiddenBranches, focusedBranch] as const,
  /** Prefix: every branch-visibility view of one repository's graph. */
  logAll: (repoId: string) => ['log', repoId] as const,
  status: (repoId: string) => ['status', repoId] as const,
  /** Tab badge totals. Cheap counterpart to `status`; see `useRepoTabStatus`. */
  repoCounts: (repoId: string) => ['repoCounts', repoId] as const,
  branches: (repoId: string) => ['branches', repoId] as const,
  tags: (repoId: string) => ['tags', repoId] as const,
  remoteTags: (repoId: string, remote: string) => ['remoteTags', repoId, remote] as const,
  remotes: (repoId: string) => ['remotes', repoId] as const,
  stashes: (repoId: string) => ['stashes', repoId] as const,
  submodules: (repoId: string) => ['submodules', repoId] as const,
  worktrees: (repoId: string) => ['worktrees', repoId] as const,
  commitDetail: (repoId: string, sha: string) => ['commit', repoId, sha] as const,
  fileDiff: (repoId: string, path: string, source: DiffSource) =>
    ['diff', repoId, path, source] as const,
  fileHistory: (repoId: string, path: string) => ['fileHistory', repoId, path] as const,
  fileBlame: (repoId: string, path: string, sha: string | null) =>
    ['fileBlame', repoId, path, sha] as const,
  fileContent: (repoId: string, path: string, sha: string | null) =>
    ['fileContent', repoId, path, sha] as const,
  mergeState: (repoId: string) => ['mergeState', repoId] as const,
  conflict: (repoId: string, path: string) => ['conflict', repoId, path] as const,
  openspecStatus: (repoId: string) => ['openspecStatus', repoId] as const,
  openspecChanges: (repoId: string) => ['openspecChanges', repoId] as const,
  openspecArchived: (repoId: string) => ['openspecArchived', repoId] as const,
  openspecArchivedChange: (repoId: string, changeId: string) =>
    ['openspecArchived', repoId, changeId] as const,
  openspecHistory: (repoId: string, changeId: string) =>
    ['openspecHistory', repoId, changeId] as const,
  /** Prefix: every change's history for one repo. */
  openspecHistoryAll: (repoId: string) => ['openspecHistory', repoId] as const,
  mehenOverview: () => ['mehenOverview'] as const,
  /** Keyed on the checked-out branch's position, so it refreshes whenever what a push would send changes. */
  mehenPushNote: (repoId: string, branch: string | null, tip: string | null, ahead: number) =>
    ['mehenPushNote', repoId, branch, tip, ahead] as const,
  mehenPushNoteAll: (repoId: string) => ['mehenPushNote', repoId] as const,
  /** Prefix: every repository's push note, refreshed when Mehen writes a new summary. */
  mehenPushNoteEvery: () => ['mehenPushNote'] as const,
  specLink: (repoId: string, branch: string) => ['specLink', repoId, branch] as const,
  /** Prefix: every branch's spec link for one repo. */
  specLinkAll: (repoId: string) => ['specLink', repoId] as const,

  /** Prefix: every page of the Agent Desk session list, regardless of filter/cursor. */
  agentSessionsAll: ['agentSessions'] as const,
  agentSessions: (filterKey: string) => ['agentSessions', filterKey] as const,
  agentSession: (sessionId: string) => ['agentSession', sessionId] as const,
  agentSessionUsage: (sessionId: string) => ['agentSessionUsage', sessionId] as const,
  /** Which AI tools are installed. Not cached across opens: installing one is
   *  exactly what a user does after finding it missing here. */
  agentProviders: (sessionId: string | null) => ['agentProviders', sessionId ?? 'none'] as const,
  /** The context a lead agent reads for an `openSpecChange`/`openSpecTask` session source. */
  agentSessionOpenspecContext: (sessionId: string) => ['agentSessionOpenspecContext', sessionId] as const,
  /** Active/archived/moved/deleted status of an `openSpecChange`/`openSpecTask` session source. */
  agentSessionOpenspecStatus: (sessionId: string) => ['agentSessionOpenspecStatus', sessionId] as const,
  /** tasks.md 2.4: whether the OpenSpec source changed since the most recent execution read it. */
  agentSessionOpenspecContextDrift: (sessionId: string) => ['agentSessionOpenspecContextDrift', sessionId] as const,
  /** Review/landing: every result record for one session (agent-desk-review-and-landing). */
  agentResults: (sessionId: string) => ['agentResults', sessionId] as const,

  /** Agent Setup: the cross-client skill/MCP inventory, scoped to a repo (or personal-only when null). */
  agentConfigInventory: (repoId: string | null) => ['agentConfigInventory', repoId] as const,
  agentConfigRecentOperations: () => ['agentConfigRecentOperations'] as const,
  agentIntentPolicy: (intent: string) => ['agentIntentPolicy', intent] as const,
  /** Agent Setup: which agent clients were detected on this machine, scoped to a repo (or null). */
  agentConfigDetectedClients: (repoId: string | null) => ['agentConfigDetectedClients', repoId] as const,

  /** External chat import: adapter detection/enabled list (agent-desk-external-chat-import). */
  agentImportAdapters: ['agentImportAdapters'] as const,
  /** External chat import: which adapters are set to keep in sync. */
  agentImportSyncPreferences: ['agentImportSyncPreferences'] as const,
  /** External chat import: one adapter's scanned external sessions. */
  agentImportScan: (adapterId: string) => ['agentImportScan', adapterId] as const,
  /** External chat import: whether/how a specific external session can be continued externally. */
  agentImportContinuation: (adapterId: string, externalSessionId: string) =>
    ['agentImportContinuation', adapterId, externalSessionId] as const,

  /**
   * Prefixes for invalidating every entry of a kind for one repo, regardless of
   * the trailing segments. Use these instead of hand-writing a shorter array:
   * a literal bypasses the factory, so it silently stops matching if a key ever
   * gains or reorders a segment.
   */
  remoteTagsAll: (repoId: string) => ['remoteTags', repoId] as const,
  fileDiffAll: (repoId: string) => ['diff', repoId] as const,
}

/**
 * Repos with a merge/cherry-pick/revert running right now, by id.
 *
 * A git operation is several file writes, not one: `cherrypick` drops
 * CHERRY_PICK_HEAD and MERGE_MSG and rewrites the working tree, and only then
 * does the command commit the result and clear that state. Every one of those
 * writes wakes the file watcher, so a refetch landing mid-operation reads a
 * half-finished repository and caches "merge in progress, no conflicts" as
 * though it were the outcome -- which is what put a stale "ready to commit"
 * banner on screen after a pick that had already committed.
 *
 * Deliberately a module-level set rather than store or ref state: the watcher
 * callback and the mutations both need it, it must be readable synchronously
 * from inside an event handler, and a change to it should never re-render
 * anything. The mutation that adds an id is responsible for removing it and
 * invalidating afterwards, so the settled state is always read exactly once.
 */
const operationsInFlight = new Set<string>()

/** Marks an operation as running; returns the matching release function. */
export function beginGitOperation(repoId: string): () => void {
  operationsInFlight.add(repoId)
  return () => operationsInFlight.delete(repoId)
}

export function isGitOperationInFlight(repoId: string): boolean {
  return operationsInFlight.has(repoId)
}

/**
 * Refresh everything derived from a repo's `openspec/` folder.
 *
 * Called both after our own writes and when the watcher reports an external
 * edit, because the files are the state: an agent or editor ticking a task has
 * to move the same counts our own click does.
 */
/**
 * Refresh everything a kept, undone or committed agent result can change.
 *
 * Keep/Undo/Commit used to refresh only the result list, so the graph panel,
 * the session row, the OpenSpec task list, the working-changes view and the
 * commit graph all kept showing the world from before the click until
 * something else happened to poke them. The list below is the honest
 * blast radius of landing agent work: the session (its state, graph nodes
 * and unread flag), the OpenSpec surfaces (the task may just have been
 * ticked), and the repository views that read the working tree and HEAD.
 */
export function invalidateAfterResultLanding(qc: QueryClient, repoId: string, sessionId: string) {
  qc.invalidateQueries({ queryKey: keys.agentResults(sessionId) })
  qc.invalidateQueries({ queryKey: keys.agentSession(sessionId) })
  qc.invalidateQueries({ queryKey: keys.agentSessionsAll })
  qc.invalidateQueries({ queryKey: keys.agentSessionOpenspecContext(sessionId) })
  qc.invalidateQueries({ queryKey: keys.agentSessionOpenspecStatus(sessionId) })
  qc.invalidateQueries({ queryKey: keys.agentSessionOpenspecContextDrift(sessionId) })
  qc.invalidateQueries({ queryKey: keys.status(repoId) })
  qc.invalidateQueries({ queryKey: keys.repoCounts(repoId) })
  qc.invalidateQueries({ queryKey: keys.log(repoId) })
  qc.invalidateQueries({ queryKey: keys.branches(repoId) })
  qc.invalidateQueries({ queryKey: keys.worktrees(repoId) })
  qc.invalidateQueries({ queryKey: keys.fileDiffAll(repoId) })
  invalidateOpenspec(qc, repoId)
}

export function invalidateOpenspec(qc: QueryClient, repoId: string) {
  qc.invalidateQueries({ queryKey: keys.openspecStatus(repoId) })
  qc.invalidateQueries({ queryKey: keys.openspecChanges(repoId) })
  qc.invalidateQueries({ queryKey: keys.openspecArchived(repoId) })
  qc.invalidateQueries({ queryKey: keys.openspecHistoryAll(repoId) })
}

/**
 * Drop all but the first page of the commit log before it refetches.
 *
 * An infinite query refetches every page it is holding, one after another,
 * because each page's offset comes from the page before it. `get_log` also
 * re-walks history from the start to reach its offset, so page five costs five
 * times page one. Together that makes a refresh of a deeply scrolled graph slow
 * enough to lag visibly behind the toast and the file list -- the graph appears
 * to update late even though it was invalidated at the same moment.
 *
 * Trimming first means the refresh is always one page. The graph snaps back to
 * the top of history, which is where a rewind, commit or merge has just moved
 * things anyway, and scrolling down reloads the rest on demand.
 */
export function trimLogToFirstPage(qc: QueryClient, repoId: string) {
  qc.setQueriesData<{ pages: unknown[]; pageParams: unknown[] }>(
    { queryKey: keys.logAll(repoId) },
    (data) => {
    if (!data || data.pages.length <= 1) return data
    return { pages: data.pages.slice(0, 1), pageParams: data.pageParams.slice(0, 1) }
    },
  )
}

/** Unwraps tauri-specta's Result<T, string> into T-or-throw for TanStack Query. */
export function unwrap<T>(result: { status: 'ok'; data: T } | { status: 'error'; error: string }): T {
  if (result.status === 'error') throw new Error(result.error)
  return result.data
}
