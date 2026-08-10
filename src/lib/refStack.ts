import type { RefInfo } from '@/lib/bindings'

/** Tags shown beside a collapsed synced pair before the stack takes over. */
export const MAX_INLINE_TAGS = 2

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
export function syncedPair(refs: RefInfo[]): SyncedPair | null {
  // Only branch-ish refs decide whether this is a plain synced pair; tags ride
  // along. Splitting first is what lets `main + origin/main + v1.2.3` collapse.
  const branchish = refs.filter((r) => r.type !== 'tag')
  if (branchish.length !== 2) return null
  const local = branchish.find((r) => r.type === 'head' || r.type === 'branch')
  const remote = branchish.find((r) => r.type === 'remote')
  if (!local || !remote) return null
  // Same branch name on both sides, or they merely happen to share a commit.
  if (shortName(remote) !== local.name) return null
  const tags = refs.filter((r) => r.type === 'tag')
  // Past a couple of tags the row stops being readable and the stack earns its
  // place again -- release commits carrying `v1.2.3` plus moving pointers like
  // `latest` are better served by the popover than by five chips in a 138px
  // column.
  if (tags.length > MAX_INLINE_TAGS) return null
  return { local, remote, rest: tags }
}
