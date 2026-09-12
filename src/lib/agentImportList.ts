import type { ScannedExternalSession } from '@/lib/bindings'

/**
 * Filtering, grouping, and bulk-selection logic for the "Import chats"
 * surface.
 *
 * Pure and component-free so it is covered by this project's Node-env vitest
 * setup, which runs `src` test files with no DOM -- the only way selection and
 * grouping behavior is testable here.
 *
 * `ImportList.tsx` turns the flat `ImportRow[]` this produces into virtualized
 * DOM and never recomputes any of it -- the same split
 * `agentSessionGrouping.ts`/`VirtualSessionList.tsx` already uses for the chat
 * sidebar, so both lists group by day the same way.
 */

/** One flattened row for the virtualizer: a day header or a chat. */
export type ImportRow =
  | { kind: 'header'; id: string; label: string; count: number; collapsed: boolean }
  | { kind: 'session'; id: string; session: ScannedExternalSession }

/** Day-bucket labels, emitted in this fixed order. Deliberately the same
 * vocabulary the chat sidebar uses (`agentSessionGrouping.ts`) so "Today"
 * means the same thing on both sides of the app. */
const DAY_BUCKETS = ['Today', 'Yesterday', 'This week', 'Earlier'] as const
export type DayBucket = (typeof DAY_BUCKETS)[number]

function dayStart(epochMs: number): number {
  const d = new Date(epochMs)
  d.setHours(0, 0, 0, 0)
  return d.getTime()
}

/**
 * Which day bucket a chat's own last-changed time falls into.
 *
 * An undated chat sorts into "Earlier" rather than "Today". Some clients
 * record no date at all (the adapters write a sentinel for it), and calling
 * those today would put a years-old conversation at the top of the list every
 * time it is opened.
 */
export function importDayBucket(updatedAt: string, now: number = Date.now()): DayBucket {
  const updated = Date.parse(updatedAt)
  if (Number.isNaN(updated)) return 'Earlier'
  const dayMs = 24 * 60 * 60 * 1000
  const diffDays = Math.floor((dayStart(now) - dayStart(updated)) / dayMs)
  if (diffDays <= 0) return 'Today'
  if (diffDays === 1) return 'Yesterday'
  if (diffDays <= 7) return 'This week'
  return 'Earlier'
}

/** What the project filter is currently narrowed to. `all` is every chat;
 * `unresolved` is the chats whose recorded folder GitWyrm could not find,
 * which is the group a person most often wants to deal with separately. */
export type ProjectFilter = { kind: 'all' } | { kind: 'unresolved' } | { kind: 'path'; path: string }

/** The project a chat belongs to, as a stable grouping key. Chats with no
 * recorded project share one key so they can be filtered as a group. */
function projectKey(session: ScannedExternalSession): string {
  switch (session.project.kind) {
    case 'resolved':
      return session.project.repoPath
    case 'unresolved':
      return session.project.recordedPath
    case 'noProjectRecorded':
      return ''
  }
}

/** The name to show for one project filter choice. */
export function projectFilterName(session: ScannedExternalSession): string {
  switch (session.project.kind) {
    case 'resolved':
      return session.project.repoName
    case 'unresolved':
      return session.project.recordedPath
    case 'noProjectRecorded':
      return 'No project recorded'
  }
}

export interface ProjectChoice {
  /** The `path` a `ProjectFilter` of kind `path` carries. */
  path: string
  name: string
  count: number
  /** True when GitWyrm could not find this folder, so the picker can say so. */
  unresolved: boolean
}

/**
 * Every project present in a scan, with how many chats each has, most chats
 * first and ties broken by name so the order does not shuffle between scans.
 */
export function projectChoices(sessions: readonly ScannedExternalSession[]): ProjectChoice[] {
  const byKey = new Map<string, ProjectChoice>()
  for (const s of sessions) {
    const path = projectKey(s)
    const existing = byKey.get(path)
    if (existing) {
      existing.count += 1
      continue
    }
    byKey.set(path, {
      path,
      name: projectFilterName(s),
      count: 1,
      unresolved: s.project.kind === 'unresolved',
    })
  }
  return [...byKey.values()].sort((a, b) => b.count - a.count || a.name.localeCompare(b.name))
}

/** Whether one chat matches the typed search. Matches on title and on the
 * project's visible name, because "the chats about GitWyrm" is as natural a
 * search as one by title, and the project name is on screen next to it. */
function matchesSearch(session: ScannedExternalSession, needle: string): boolean {
  if (!needle) return true
  const q = needle.toLowerCase()
  return (
    session.summary.title.toLowerCase().includes(q) ||
    projectFilterName(session).toLowerCase().includes(q)
  )
}

function matchesProject(session: ScannedExternalSession, filter: ProjectFilter): boolean {
  switch (filter.kind) {
    case 'all':
      return true
    case 'unresolved':
      return session.project.kind === 'unresolved'
    case 'path':
      return projectKey(session) === filter.path
  }
}

/** Hide chats already brought in, for someone working through a long backlog. */
export type ImportedFilter = 'all' | 'notImported'

export interface FilterOptions {
  search?: string
  project?: ProjectFilter
  imported?: ImportedFilter
}

/**
 * The chats one set of filters leaves, newest first.
 *
 * Sorting here rather than trusting scan order: the adapters sort newest-first
 * already, but this list is also fed by a re-scan that can arrive in a
 * different order, and a list that reorders itself under a cursor mid
 * shift-select is how the wrong chats get imported.
 */
export function filterImportSessions(
  sessions: readonly ScannedExternalSession[],
  { search = '', project = { kind: 'all' }, imported = 'all' }: FilterOptions = {}
): ScannedExternalSession[] {
  const needle = search.trim()
  return sessions
    .filter(
      (s) =>
        matchesSearch(s, needle) &&
        matchesProject(s, project) &&
        (imported === 'all' || !s.importedSessionId)
    )
    .sort((a, b) => {
      const at = Date.parse(a.summary.updatedAt)
      const bt = Date.parse(b.summary.updatedAt)
      // Undated chats sink rather than sorting as epoch zero *or* as now --
      // either would scatter them through the dated ones.
      if (Number.isNaN(at) && Number.isNaN(bt)) return 0
      if (Number.isNaN(at)) return 1
      if (Number.isNaN(bt)) return -1
      return bt - at
    })
}

/**
 * Flattens filtered chats into day-grouped, virtualizer-ready rows.
 *
 * A header stays visible for a collapsed group so it can always be expanded
 * again, exactly as the chat sidebar's own grouping does.
 */
export function buildImportRows(
  sessions: readonly ScannedExternalSession[],
  { collapsedGroupIds, now = Date.now() }: { collapsedGroupIds: ReadonlySet<string>; now?: number }
): ImportRow[] {
  const buckets = new Map<DayBucket, ScannedExternalSession[]>()
  for (const s of sessions) {
    const bucket = importDayBucket(s.summary.updatedAt, now)
    const list = buckets.get(bucket)
    if (list) list.push(s)
    else buckets.set(bucket, [s])
  }

  const rows: ImportRow[] = []
  for (const bucket of DAY_BUCKETS) {
    const list = buckets.get(bucket)
    if (!list || list.length === 0) continue
    const collapsed = collapsedGroupIds.has(bucket)
    rows.push({ kind: 'header', id: bucket, label: bucket, count: list.length, collapsed })
    if (collapsed) continue
    for (const s of list) {
      rows.push({ kind: 'session', id: s.summary.externalSessionId, session: s })
    }
  }
  return rows
}

/** A click's modifier keys, in the same shape the commit list uses. */
export interface SelectModifiers {
  shift: boolean
  ctrl: boolean
}

/**
 * The next selection after a click, following the commit list's grammar
 * (`views/GraphView.tsx`): Shift ranges from the anchor, Ctrl/Cmd toggles one,
 * a plain click selects just that one.
 *
 * `order` is the ids of the chats currently on screen, in display order, so a
 * Shift-range covers what the person can actually see rather than chats a
 * filter is hiding.
 *
 * Differs from the commit list in one place, deliberately. A plain click there
 * deselects when it lands on the only selected row; here it always selects,
 * because these rows feed a bulk action rather than moving a cursor --
 * clicking a checked box and losing the whole selection would be a surprise.
 * Clearing has its own visible control.
 */
export function nextSelection(
  clickedId: string,
  { shift, ctrl }: SelectModifiers,
  {
    order,
    selected,
    anchor,
  }: { order: readonly string[]; selected: ReadonlySet<string>; anchor: string | null }
): { selected: string[]; anchor: string | null } {
  const position = new Map(order.map((id, i) => [id, i]))

  if (shift && anchor && position.has(anchor) && position.has(clickedId)) {
    const a = position.get(anchor)!
    const b = position.get(clickedId)!
    const [lo, hi] = a < b ? [a, b] : [b, a]
    const range = order.slice(lo, hi + 1)
    // The range joins what was already selected rather than replacing it, so
    // two separate stretches can be gathered. The anchor stays put so the same
    // range can be widened or narrowed by shift-clicking again.
    return { selected: inOrder(new Set([...selected, ...range]), position), anchor }
  }

  if (ctrl) {
    const next = new Set(selected)
    if (next.has(clickedId)) {
      next.delete(clickedId)
      const remaining = inOrder(next, position)
      // Removing the anchor hands the role to whatever is still selected, so
      // the next Shift-click still has somewhere to range from.
      const nextAnchor = anchor === clickedId ? (remaining[remaining.length - 1] ?? null) : anchor
      return { selected: remaining, anchor: nextAnchor }
    }
    next.add(clickedId)
    return { selected: inOrder(next, position), anchor: clickedId }
  }

  return { selected: [clickedId], anchor: clickedId }
}

/** Selection kept in display order so a batch is imported in the order it is
 * shown, and the count never disagrees with the list. Ids no longer on screen
 * (filtered away since selection) are dropped. */
function inOrder(ids: ReadonlySet<string>, position: ReadonlyMap<string, number>): string[] {
  return [...ids]
    .filter((id) => position.has(id))
    .sort((a, b) => (position.get(a) ?? 0) - (position.get(b) ?? 0))
}

/**
 * Select-all / clear for the chats currently on screen.
 *
 * Scoped to what is visible, not to the whole scan: someone who has searched
 * for "migration" and presses this means those chats, and a select-all
 * reaching past the filter into hundreds of unseen chats is how a
 * two-hundred-chat import happens by accident.
 */
export function toggleSelectAllVisible(
  visibleIds: readonly string[],
  selected: ReadonlySet<string>
): { selected: string[]; anchor: string | null } {
  const allSelected = visibleIds.length > 0 && visibleIds.every((id) => selected.has(id))
  const next = new Set(selected)
  if (allSelected) {
    for (const id of visibleIds) next.delete(id)
    // Keeps a selection made under a different filter, so clearing a search
    // does not silently discard chats chosen before it.
    return { selected: [...next], anchor: null }
  }
  for (const id of visibleIds) next.add(id)
  return { selected: [...next], anchor: visibleIds[visibleIds.length - 1] ?? null }
}

/** Whether a chat has already been brought in. */
export function isImported(session: ScannedExternalSession): boolean {
  return session.importedSessionId != null
}

/**
 * What one press will actually do, given a selection.
 *
 * Split because the two halves need different sentences: chats never brought
 * in are created, chats already in are refreshed with whatever is new. A
 * button that said "Import 24 chats" when 19 of them were refreshes would be
 * describing work it is not doing.
 */
export function summarizeSelection(
  selectedIds: readonly string[],
  sessions: readonly ScannedExternalSession[]
): { total: number; newChats: number; refreshes: number } {
  const byId = new Map(sessions.map((s) => [s.summary.externalSessionId, s]))
  let refreshes = 0
  let total = 0
  for (const id of selectedIds) {
    const session = byId.get(id)
    if (!session) continue
    total += 1
    if (isImported(session)) refreshes += 1
  }
  return { total, newChats: total - refreshes, refreshes }
}
