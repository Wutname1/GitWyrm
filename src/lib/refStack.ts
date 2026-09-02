import type { RefInfo } from '@/lib/bindings'

/** Tags shown beside a collapsed synced pair before the stack takes over. */
export const MAX_INLINE_TAGS = 2

/**
 * Chips drawn inline before the stack takes over.
 *
 * Two synced branches (`edge` + `main`) is the shape this exists for; past
 * three the row stops fitting a 138px column and the popover reads better.
 */
export const MAX_INLINE_CHIPS = 3

export function remoteName(refTag: RefInfo): string | null {
  return refTag.type === 'remote' ? refTag.name.split('/')[0] : null
}

export function shortName(refTag: RefInfo): string {
  const remote = remoteName(refTag)
  return remote ? refTag.name.slice(remote.length + 1) : refTag.name
}

export interface SyncedPair {
  local: RefInfo
  remote: RefInfo
  /** Tags to draw beside the collapsed pair, in the order they arrived. */
  rest: RefInfo[]
}

/** One chip's worth of refs: a branch, plus the remote it is synced with. */
export interface RefGroup {
  /** The ref the chip is named after. */
  primary: RefInfo
  /** The remote-tracking ref folded into `primary`, when there is exactly one. */
  syncedWith: RefInfo | null
}

export interface GroupedRefs {
  /** Branch chips to draw in order, each already collapsed where it can be. */
  groups: RefGroup[]
  /** Tags to draw beside them, in the order they arrived. */
  tags: RefInfo[]
}

/**
 * How a local branch finds its remote counterpart.
 *
 * `upstream` is the real tracking relationship read from git config; the name
 * match is only a fallback for a branch that has never been linked. Keeping
 * them apart matters -- a local `foo` whose upstream is `origin/bar` is a real
 * pair that a name match misses, and two same-named refs that merely share a
 * commit are not a pair at all. Same distinction the backend draws between
 * `local_counterpart` and `tracked_by`.
 */
export type UpstreamOf = (localName: string) => string | null | undefined

/**
 * Fold a commit's refs into one chip per branch, collapsing each local branch
 * with its own remote-tracking ref.
 *
 * There is no choice to make between `main` and `origin/main`, so listing both
 * is noise -- and a commit carrying two such pairs (`edge`, `origin/edge`,
 * `main`, `origin/main`) was showing four rows in a popover for what is really
 * two branches. Each pair collapses to a single chip with a small remote glyph.
 *
 * A branch keeps its own chip when there is nothing to fold: no counterpart, or
 * more than one remote claiming it (two remotes really are a choice).
 */
export function groupRefs(refs: RefInfo[], upstreamOf?: UpstreamOf): GroupedRefs {
  const tags = refs.filter((r) => r.type === 'tag')
  const locals = refs.filter((r) => r.type === 'head' || r.type === 'branch')
  const remotes = refs.filter((r) => r.type === 'remote')

  const claimed = new Set<RefInfo>()
  const groups: RefGroup[] = locals.map((local) => {
    // Prefer the configured upstream; fall back to the same short name.
    const tracked = upstreamOf?.(local.name)
    const matches = remotes.filter((r) =>
      tracked ? r.name === tracked : shortName(r) === local.name
    )
    // Exactly one counterpart is a pair. Two remotes offering the same branch is
    // a real choice, so both stay visible.
    const only = matches.length === 1 && !claimed.has(matches[0]) ? matches[0] : null
    if (only) claimed.add(only)
    return { primary: local, syncedWith: only }
  })

  // Remote branches with no local counterpart still need a chip of their own.
  for (const r of remotes) {
    if (!claimed.has(r)) groups.push({ primary: r, syncedWith: null })
  }

  return { groups, tags }
}

/**
 * The common "everything is in sync" case: one local branch (checked out or
 * not) plus its remote-tracking ref, both on this commit. There is no choice to
 * make between those two, so a `+1` stack that opens a 288px popover to list two
 * rows with the same name is more UI than the fact deserves -- it collapses to a
 * single chip with a small remote glyph instead.
 *
 * Tags on the same commit do NOT block the collapse. A release commit almost
 * always carries `main`, `origin/main` and `v1.2.3` together, and folding the
 * branch pair into a `+2` chip hides the branch name behind a popover for what
 * is the single most ordinary shape in the graph. The tags come back as their
 * own chips beside the pair, which is both fewer clicks and more information.
 *
 * Returns the local ref, the remote it is synced with, and the tags left over,
 * or null when the branch refs are more interesting than a single synced pair
 * (extra branches, two remotes, a remote with no local counterpart).
 */
export function syncedPair(refs: RefInfo[], upstreamOf?: UpstreamOf): SyncedPair | null {
  const { groups, tags } = groupRefs(refs, upstreamOf)
  if (groups.length !== 1) return null
  const [only] = groups
  if (!only.syncedWith) return null
  // Past a couple of tags the row stops being readable and the stack earns its
  // place again -- release commits carrying `v1.2.3` plus moving pointers like
  // `latest` are better served by the popover than by five chips in a 138px
  // column.
  if (tags.length > MAX_INLINE_TAGS) return null
  return { local: only.primary, remote: only.syncedWith, rest: tags }
}
