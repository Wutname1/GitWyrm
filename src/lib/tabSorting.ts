import { normalizePath } from '@/lib/paths'
import type {
  TabGroup,
  TabOrderItem,
  TabSort,
  TabSortDirection,
} from '@/stores/workspaceStore'

/**
 * How many changes a repo tab is reporting, keyed by repo path. Tabs publish
 * their own counts (see `useTabChangeCounts`) because the numbers come from
 * per-repo queries that only the tab components subscribe to.
 */
export type TabChangeCounts = Record<string, number>

function pathKey(path: string): string {
  return normalizePath(path).toLowerCase()
}

/** Display name for sorting: the user's tab alias if set, else the folder name. */
function sortName(
  path: string,
  aliases: Record<string, string>,
  names: Record<string, string>,
): string {
  const key = pathKey(path)
  return (aliases[path] ?? names[key] ?? path.split(/[\\/]/).filter(Boolean).at(-1) ?? path)
    .toLowerCase()
}

/** When each tab was last switched to, epoch ms keyed by lowercased path. */
export type TabLastUsedAt = Record<string, number>

export interface ArrangeOptions {
  order: TabOrderItem[]
  groups: TabGroup[]
  sort: TabSort
  /** Which way the rule runs. Defaults to forward; 'manual' ignores it. */
  direction?: TabSortDirection
  pinned: string[]
  /** Tab aliases keyed by repo path, so Name sorts by what the user sees. */
  aliases: Record<string, string>
  /** Folder names keyed by lowercased path, for repos with no alias. */
  names: Record<string, string>
  /** Uncommitted + unpushed counts keyed by lowercased path. */
  changeCounts: TabChangeCounts
  /** Last-used stamps keyed by lowercased path, for the Recently used sort. */
  lastUsedAt?: TabLastUsedAt
}

/**
 * The arrangement the tab strip renders: pinned tabs first (in pin order), then
 * everything else in the chosen sort. `tabOrder` is never rewritten, so
 * switching back to Manual restores the order the user dragged into place.
 *
 * Groups move as single units. A group sorts by its best member -- alphabetically
 * first for Name, most changes for Changes -- so a group never splits apart and
 * never sinks below a loose tab it outranks.
 */
/**
 * Which heading a tab sits under in the Recently used strip. `id` is stable for
 * React keys and grouping; `label` is what the user reads.
 */
export interface TabRecencyBucket {
  id: string
  label: string
}

/** Start of the calendar day containing `time`, in the viewer's own timezone. */
function startOfDay(time: number): number {
  const date = new Date(time)
  date.setHours(0, 0, 0, 0)
  return date.getTime()
}

const DAY_MS = 86_400_000

/**
 * The heading a tab belongs under, given when it was last used and when "now"
 * is. Buckets are calendar days in the viewer's timezone, not rolling 24-hour
 * windows -- something used last night is "Yesterday" all of today, which is
 * how people actually talk about it.
 *
 * Days 2-6 get their own dated heading. Day 7 and beyond collapse into one
 * bucket, so the strip never grows an unbounded run of headings.
 */
export function recencyBucket(
  lastUsed: number | undefined,
  now: number,
): TabRecencyBucket {
  // No stamp means the tab has not been opened since we started keeping track,
  // which is indistinguishable from long-ago -- so it files with the oldest.
  if (lastUsed == null || !Number.isFinite(lastUsed) || lastUsed <= 0)
    return { id: 'older', label: 'Over a Week or Older' }

  const today = startOfDay(now)
  // A stamp in the future (clock change, edited settings) is still "now" to the
  // user; clamping keeps it at the top instead of exiling it to the old bucket.
  const daysAgo = Math.max(0, Math.round((today - startOfDay(lastUsed)) / DAY_MS))

  if (daysAgo === 0) return { id: 'today', label: 'Today' }
  if (daysAgo === 1) return { id: 'yesterday', label: 'Yesterday' }
  if (daysAgo >= 7) return { id: 'older', label: 'Over a Week or Older' }

  const date = new Date(lastUsed)
  return {
    id: `day-${startOfDay(lastUsed)}`,
    // e.g. "Mon, Aug 11" -- the weekday is what makes a recent date readable.
    label: date.toLocaleDateString(undefined, {
      weekday: 'short',
      month: 'short',
      day: 'numeric',
    }),
  }
}

/**
 * Split an already-sorted Recently-used arrangement into its headed sections.
 * Only buckets that actually contain tabs come back, in strip order, so the
 * strip never renders an empty heading.
 */
export function bucketByRecency(
  items: TabOrderItem[],
  options: {
    groups: TabGroup[]
    lastUsedAt: TabLastUsedAt
    now: number
  },
): { bucket: TabRecencyBucket; items: TabOrderItem[] }[] {
  const { groups, lastUsedAt, now } = options
  const sections: { bucket: TabRecencyBucket; items: TabOrderItem[] }[] = []

  for (const item of items) {
    const paths =
      item.type === 'repo'
        ? [item.path]
        : (groups.find((group) => group.id === item.id)?.repoPaths ?? [])
    const newest = paths.reduce(
      (best, path) => Math.max(best, lastUsedAt[pathKey(path)] ?? 0),
      0,
    )
    const bucket = recencyBucket(newest || undefined, now)
    // Items arrive in sort order, so a bucket's run is contiguous -- but only
    // append to the last section when it matches, never merge back into an
    // earlier one, or a reversed sort would fold two runs together.
    const current = sections.at(-1)
    if (current && current.bucket.id === bucket.id) current.items.push(item)
    else sections.push({ bucket, items: [item] })
  }

  return sections
}

export function arrangeTabs(options: ArrangeOptions): {
  pinned: TabOrderItem[]
  rest: TabOrderItem[]
} {
  const { order, groups, sort, pinned, aliases, names, changeCounts } = options
  const lastUsedAt = options.lastUsedAt ?? {}
  const flip = options.direction === 'reverse' ? -1 : 1
  const pinnedKeys = new Map(pinned.map((path, index) => [pathKey(path), index]))

  const pinnedItems: TabOrderItem[] = []
  const rest: TabOrderItem[] = []
  for (const item of order) {
    // Only loose repo tabs can be pinned; toggleTabPin lifts a repo out of its
    // group first, so a group item here is never pinned.
    if (item.type === 'repo' && pinnedKeys.has(pathKey(item.path))) {
      pinnedItems.push(item)
    } else {
      rest.push(item)
    }
  }

  pinnedItems.sort(
    (left, right) =>
      (pinnedKeys.get(pathKey((left as { path: string }).path)) ?? 0) -
      (pinnedKeys.get(pathKey((right as { path: string }).path)) ?? 0),
  )

  if (sort === 'manual') return { pinned: pinnedItems, rest }

  const pathsFor = (item: TabOrderItem): string[] =>
    item.type === 'repo'
      ? [item.path]
      : (groups.find((group) => group.id === item.id)?.repoPaths ?? [])

  // A group takes its best member's name -- alphabetically first -- so it sorts
  // as a unit and never sinks below a loose tab it outranks.
  const nameFor = (item: TabOrderItem) => {
    const paths = pathsFor(item)
    if (paths.length === 0) return '￿'
    return paths.map((path) => sortName(path, aliases, names)).sort()[0]!
  }

  // A group is as recent as its most recently used member: a project should
  // rise when you work in any part of it, not sink to its stalest corner.
  const lastUsedFor = (item: TabOrderItem) =>
    pathsFor(item).reduce((newest, path) => Math.max(newest, lastUsedAt[pathKey(path)] ?? 0), 0)

  const sorted = [...rest]
  if (sort === 'name') {
    sorted.sort((left, right) => flip * nameFor(left).localeCompare(nameFor(right)))
  } else if (sort === 'recent') {
    // Recently used: the tab you touched last sits at the top and everything
    // else sinks as it goes untouched. A group takes its most recently used
    // member, so opening one repo inside a project floats the whole project.
    // Tabs never opened on this machine have no stamp and fall to the bottom,
    // where the strip labels them together.
    sorted.sort((left, right) => {
      const delta = lastUsedFor(right) - lastUsedFor(left)
      return flip * (delta !== 0 ? delta : nameFor(left).localeCompare(nameFor(right)))
    })
  } else {
    // Changes: busiest first, and tabs with the same count fall back to name so
    // the strip does not reshuffle every time two repos happen to tie. Reversing
    // flips both, making Z-A-within-quietest an exact mirror of the forward run.
    const countFor = (item: TabOrderItem) =>
      pathsFor(item).reduce((total, path) => total + (changeCounts[pathKey(path)] ?? 0), 0)
    sorted.sort((left, right) => {
      const delta = countFor(right) - countFor(left)
      return flip * (delta !== 0 ? delta : nameFor(left).localeCompare(nameFor(right)))
    })
  }

  return { pinned: pinnedItems, rest: sorted }
}
