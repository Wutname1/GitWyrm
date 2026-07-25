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
export function arrangeTabs(options: ArrangeOptions): {
  pinned: TabOrderItem[]
  rest: TabOrderItem[]
} {
  const { order, groups, sort, pinned, aliases, names, changeCounts } = options
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

  const sorted = [...rest]
  if (sort === 'name') {
    sorted.sort((left, right) => flip * nameFor(left).localeCompare(nameFor(right)))
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
