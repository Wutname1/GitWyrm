import type { AgentSessionHeader, SessionSource } from '@/lib/bindings'

/**
 * Grouping/row-flattening logic for the Agent Desk session sidebar (tasks
 * 3.1-3.4, 3.7). Kept out of any component so it can be covered by plain
 * `.test.ts` unit tests -- this project's `vitest.config.ts` runs `src/**\/*.test.ts`
 * in a Node environment with no DOM/testing-library, so component rendering is
 * not testable here; the row math that decides height/order/grouping is.
 *
 * `VirtualSessionList`/`SessionGroups` turn the flat `SidebarRow[]` this module
 * produces into virtualized DOM, but never recompute the grouping themselves.
 */

export type SidebarGroupMode = 'recent' | 'project' | 'diff'

/** One flattened row for the virtualizer: either a group header or a session. */
export type SidebarRow =
  | { kind: 'header'; id: string; label: string; count: number; collapsed: boolean }
  | {
      kind: 'session'
      id: string
      header: AgentSessionHeader
      /**
       * True when the row's group header already names the project, so the
       * row must not repeat it. Every other grouping leaves the project
       * invisible on the row, which is what the Persistent Context Rule
       * exists to prevent -- acting on the wrong repository.
       */
      projectInHeader?: boolean
    }

/** Day-bucket labels for Recent grouping, oldest-eligible-first is not required -- buckets are emitted in this fixed order. */
const RECENT_BUCKETS = ['Today', 'Yesterday', 'This week', 'Earlier'] as const
type RecentBucket = (typeof RECENT_BUCKETS)[number]

function dayStart(epochMs: number): number {
  const d = new Date(epochMs)
  d.setHours(0, 0, 0, 0)
  return d.getTime()
}

/**
 * Which Recent bucket a session's `updatedAt` falls into, relative to `now`.
 * Exported so tests can pin `now` instead of depending on the real clock.
 */
export function recentBucket(updatedAt: string, now: number = Date.now()): RecentBucket {
  const updated = Date.parse(updatedAt)
  if (Number.isNaN(updated)) return 'Earlier'
  const todayStart = dayStart(now)
  const dayMs = 24 * 60 * 60 * 1000
  const diffDays = Math.floor((todayStart - dayStart(updated)) / dayMs)
  if (diffDays <= 0) return 'Today'
  if (diffDays === 1) return 'Yesterday'
  if (diffDays <= 7) return 'This week'
  return 'Earlier'
}

/**
 * Normalizes a repo path for Project grouping so `C:\Repo`, `C:/Repo`, and
 * `c:/repo/` all land in the same group -- Windows paths vary in slash
 * direction and case, and a trailing slash must not split one project into
 * two headers. Grouping key only; never shown to the user.
 */
export function normalizeRepoPath(path: string): string {
  return path.trim().replace(/\\/g, '/').replace(/\/+$/, '').toLowerCase()
}

/**
 * Diff-group bucket: sessions with changed files (grouped by result state),
 * versus sessions with none. `changedFileCount > 0` per architecture.md's
 * `AgentSessionHeader` and tasks.md 3.4 ("based on `changed_file_count > 0`
 * and result state").
 */
export function diffBucketLabel(header: AgentSessionHeader): string {
  if (header.changedFileCount <= 0) return 'No changes yet'
  switch (header.state) {
    case 'working':
      return 'Changing files'
    case 'needsInput':
      return 'Needs your input'
    case 'finished':
      return 'Ready to review'
    case 'failed':
      return 'Failed'
    case 'stopped':
      return 'Stopped with changes'
    default:
      return 'Has changes'
  }
}

interface BuildRowsOptions {
  mode: SidebarGroupMode
  /** Group IDs the user collapsed; their session rows are omitted, header stays. */
  collapsedGroupIds: ReadonlySet<string>
  now?: number
}

/**
 * Duplicate repo names (task 3.7) are legitimate -- two different machines'
 * clones of the same repo name, or two unrelated projects sharing a folder
 * name -- so Project grouping keys on normalized *path*, not name, and the
 * header label falls back to the path when the name is blank so two
 * same-named repos never collapse into one indistinguishable group.
 */
function projectGroupKeyAndLabel(header: AgentSessionHeader): { key: string; label: string } {
  const key = normalizeRepoPath(header.repoPath || header.repoId)
  const label = header.repoName.trim() || header.repoPath || header.repoId
  return { key, label }
}

/**
 * Builds the flat, virtualizer-ready row list for one grouping mode.
 *
 * Headers always precede their sessions and always appear even when the
 * group's sessions are currently collapsed (task 3.3), so collapsing a group
 * never removes the ability to expand it again. Session order within a group
 * follows the input order, which callers keep newest-first (matching the
 * backend's index order per `useAgentSessions`'s doc comment) so nothing here
 * needs to re-sort and risk disagreeing with pagination.
 */
export function buildSidebarRows(
  headers: readonly AgentSessionHeader[],
  { mode, collapsedGroupIds, now = Date.now() }: BuildRowsOptions
): SidebarRow[] {
  if (mode === 'recent') {
    const buckets = new Map<RecentBucket, AgentSessionHeader[]>()
    for (const h of headers) {
      const bucket = recentBucket(h.updatedAt, now)
      const list = buckets.get(bucket)
      if (list) list.push(h)
      else buckets.set(bucket, [h])
    }
    const rows: SidebarRow[] = []
    for (const bucket of RECENT_BUCKETS) {
      const list = buckets.get(bucket)
      if (!list || list.length === 0) continue
      const collapsed = collapsedGroupIds.has(bucket)
      rows.push({ kind: 'header', id: bucket, label: bucket, count: list.length, collapsed })
      if (!collapsed) {
        for (const h of list) rows.push({ kind: 'session', id: h.sessionId, header: h })
      }
    }
    return rows
  }

  if (mode === 'project') {
    const order: string[] = []
    const groups = new Map<string, { label: string; sessions: AgentSessionHeader[] }>()
    for (const h of headers) {
      const { key, label } = projectGroupKeyAndLabel(h)
      let g = groups.get(key)
      if (!g) {
        g = { label, sessions: [] }
        groups.set(key, g)
        order.push(key)
      }
      g.sessions.push(h)
    }
    const rows: SidebarRow[] = []
    for (const key of order) {
      const g = groups.get(key)
      if (!g) continue
      const collapsed = collapsedGroupIds.has(key)
      rows.push({ kind: 'header', id: key, label: g.label, count: g.sessions.length, collapsed })
      if (!collapsed) {
        for (const h of g.sessions) rows.push({ kind: 'session', id: h.sessionId, header: h, projectInHeader: true })
      }
    }
    return rows
  }

  // mode === 'diff'
  const order: string[] = []
  const groups = new Map<string, AgentSessionHeader[]>()
  for (const h of headers) {
    const label = diffBucketLabel(h)
    const list = groups.get(label)
    if (list) list.push(h)
    else {
      groups.set(label, [h])
      order.push(label)
    }
  }
  const rows: SidebarRow[] = []
  for (const label of order) {
    const list = groups.get(label)
    if (!list) continue
    const collapsed = collapsedGroupIds.has(label)
    rows.push({ kind: 'header', id: label, label, count: list.length, collapsed })
    if (!collapsed) {
      for (const h of list) rows.push({ kind: 'session', id: h.sessionId, header: h })
    }
  }
  return rows
}

/**
 * Compact trailing-slot timestamp matching the mockup's row vocabulary
 * (`9m`, `2h`, `3d`) -- distinct from `gitDisplay.ts`'s `formatRelativeTime`
 * ("4h ago"), which is too wide for the 28px row's fixed trailing column.
 * Falls back to `--` for an unparseable timestamp rather than throwing, since
 * this renders directly in a virtualized list row.
 */
export function formatCompactAge(iso: string, now: number = Date.now()): string {
  const then = Date.parse(iso)
  if (Number.isNaN(then)) return '--'
  const secs = Math.max(0, Math.floor((now - then) / 1000))
  if (secs < 60) return 'now'
  const mins = Math.floor(secs / 60)
  if (mins < 60) return `${mins}m`
  const hours = Math.floor(mins / 60)
  if (hours < 24) return `${hours}h`
  const days = Math.floor(hours / 24)
  if (days < 30) return `${days}d`
  const months = Math.floor(days / 30)
  if (months < 12) return `${months}mo`
  return `${Math.floor(months / 12)}y`
}

/**
 * Resolves the `repoId` filter actually sent to `agent_session_list` from the
 * sidebar's "This project only" toggle (R4.1: repository filtering is an
 * *optional* filter, not the Desk's identity). `currentRepoId` is the main
 * window's current target -- what "this project" means when the toggle is
 * on -- and is deliberately allowed to be `null` (a window that has not
 * finished opening its repo yet): in that case the toggle cannot be honoured,
 * so the list falls back to app-wide rather than silently filtering to
 * "nothing", which would look identical to an empty workspace.
 */
export function resolveSessionRepoFilter(scopeToCurrentRepo: boolean, currentRepoId: string | null): string | null {
  if (!scopeToCurrentRepo) return null
  return currentRepoId
}

/** Display label for a session's leading kind icon slot, keyed off `SessionSource.kind`. */
export function sourceKindLabel(kind: SessionSource['kind']): string {
  // Typed as the real union, not `string`. As `string` the compiler could not
  // check this, so when `imported` was added every typed site was named and
  // this one silently fell through to 'Chat' -- an imported conversation
  // labelled the same as one started here, in the panels meant to preserve
  // where it came from.
  switch (kind) {
    case 'issue':
      return 'Issue'
    case 'pullRequest':
      return 'Pull request'
    case 'openSpecChange':
      return 'OpenSpec change'
    case 'openSpecTask':
      return 'OpenSpec task'
    case 'commit':
      return 'Commit'
    case 'diff':
      return 'Diff'
    case 'workingChanges':
      return 'Working changes'
    case 'checkFailure':
      return 'Failed check'
    case 'imported':
      return 'Imported chat'
    case 'manual':
      return 'Chat'
  }
}

/**
 * What the main window should say about agent work happening elsewhere.
 *
 * Agent Desk is a separate OS window by design, so "the user is looking at
 * something else" is the normal case rather than the edge case -- and until
 * this existed, a run could finish into a void: no badge, no notification, no
 * signal of any kind outside a sidebar in a window nobody was watching. The
 * only strategy available to a person was to keep checking.
 *
 * `needsYou` outranks `working` because it is the one that cannot make
 * progress without them.
 */
export function summarizeAgentActivity(
  headers: Array<{ state: string }>
): { tone: 'needsYou' | 'working' | null; count: number; label: string } {
  const needsYou = headers.filter((h) => h.state === 'needsInput').length
  if (needsYou > 0) {
    return {
      tone: 'needsYou',
      count: needsYou,
      label: needsYou === 1 ? '1 chat needs you' : `${needsYou} chats need you`,
    }
  }
  const working = headers.filter((h) => h.state === 'working' || h.state === 'preparing').length
  if (working > 0) {
    return {
      tone: 'working',
      count: working,
      label: working === 1 ? '1 chat is working' : `${working} chats are working`,
    }
  }
  return { tone: null, count: 0, label: '' }
}

/**
 * The points where a conversation changed hands, newest last.
 *
 * Every segment carries a label written by whichever code created it, and the
 * ones that matter say why the conversation moved: "Imported from Claude",
 * "Continued in GitWyrm", "Unlinked from ...". Those labels are the honest-
 * provenance promise made concrete -- import, continue-here and continue-
 * externally must never read as the same thing -- and until this existed
 * nothing rendered them, so a person could not tell which part of a
 * conversation came from somewhere else.
 *
 * A segment simply labelled "Conversation" marks an ordinary start and is
 * dropped: it carries no origin information and would bury the ones that do.
 * The check is an exact match rather than a substring so a future label like
 * "Conversation imported from X" would still be shown.
 */
export function conversationHandoffs<T extends { segmentId: string; label: string }>(segments: readonly T[]): T[] {
  return segments.filter((s) => s.label.trim() !== 'Conversation')
}

/**
 * What the chat list says when it has no rows to show.
 *
 * Pulled out of the JSX so the one case that was missing can be tested: a
 * FAILED read. `SessionSidebar` never destructured `isError`, so a list that
 * could not be fetched rendered the same sentence as a list that is genuinely
 * empty -- "Start a new chat..." told someone with two hundred chats that
 * they had none, which is the absence-for-a-failure inversion this app treats
 * as its sharpest class of bug.
 *
 * Order matters: a failed read is reported before anything else, because
 * every other branch is a statement about content that was never seen.
 */
export function sidebarEmptyMessage(input: {
  failed: boolean
  searchTerm?: string
  archived?: boolean
  headerCount: number
}): string {
  if (input.failed) {
    return 'GitWyrm could not load your chats. They are still here; this list could not be read just now.'
  }
  // A search that finds nothing must not read as "you have no chats" -- that
  // tells someone with two hundred of them to start another one.
  if (input.searchTerm) return `No chats match "${input.searchTerm}".`
  if (input.archived) {
    return 'No archived chats. Archiving one puts it here, and you can put it back any time.'
  }
  if (input.headerCount === 0) {
    return 'Start a new chat, or open an issue, pull request, or OpenSpec task from the main window and choose an AI action.'
  }
  return 'Nothing matches this grouping yet.'
}
