import type { BranchInfo, RemoteInfo } from '@/lib/bindings'

/**
 * One branch, wherever it lives.
 *
 * The sidebar keeps local branches and remote branches in separate sections,
 * which is fine for browsing but wrong for managing: deciding whether a branch
 * can go means looking at both copies at once. A row here is the branch, with
 * whichever copies exist attached.
 */
export interface BranchRow {
  /** Branch name with no remote prefix -- the name a person would say. */
  name: string
  /** The copy on this computer, when there is one. */
  local: BranchInfo | null
  /** The remote holding a copy, when there is one. */
  remote: string | null
  /** True when this is the branch currently checked out. */
  isCurrent: boolean
  /** Newest tip time across the copies, epoch seconds, for staleness sorting. */
  time: number | null
  /** Commits on the remote that this computer does not have. */
  behind: number
  /** Commits here that the remote does not have. */
  ahead: number
  /** Never sent to a remote, so deleting it loses the only copy. */
  neverPushed: boolean
  /** The remote it tracked is gone, so it is a likely cleanup candidate. */
  upstreamGone: boolean
}

/** What a row can have done to it, given where its copies live. */
export interface RowCapabilities {
  /** A fast-forward is possible: behind with nothing of its own to lose. */
  canPull: boolean
  /** Deleting would destroy work that exists nowhere else. */
  losesWork: boolean
}

function shortRemoteName(full: string): string {
  const slash = full.indexOf('/')
  return slash === -1 ? full : full.slice(slash + 1)
}

/**
 * Join local branches and remote branches into one row per branch.
 *
 * Pairing prefers the configured upstream and falls back to a matching name,
 * the same rule the graph's ref chips use. The two genuinely disagree -- a
 * local branch whose upstream points at a differently-named remote branch is a
 * real pair that a name match misses, and two same-named branches that were
 * never linked are not a pair at all.
 */
export function buildBranchRows(
  branches: BranchInfo[],
  remotes: RemoteInfo[],
): BranchRow[] {
  const rows = new Map<string, BranchRow>()

  for (const local of branches) {
    const sync = local.sync
    rows.set(local.name, {
      name: local.name,
      local,
      remote: local.upstream ? (local.upstream.split('/')[0] ?? null) : null,
      isCurrent: local.is_head,
      time: local.time,
      behind: sync.kind === 'diverged' ? sync.behind : 0,
      ahead: sync.kind === 'diverged' ? sync.ahead : 0,
      neverPushed: sync.kind === 'never_pushed',
      upstreamGone: sync.kind === 'upstream_gone',
    })
  }

  for (const remote of remotes) {
    for (const branch of remote.branches) {
      const short = shortRemoteName(branch.name)
      // `tracked_by` is the real link read from config; the name match is only a
      // fallback for a branch that was never connected to anything.
      const owner = branch.tracked_by ?? (rows.has(short) ? short : null)
      const existing = owner ? rows.get(owner) : undefined
      if (existing) {
        // A local row already covers this branch; record where its copy lives.
        existing.remote ??= remote.name
        existing.time = Math.max(existing.time ?? 0, branch.time ?? 0) || existing.time
        continue
      }
      // Remote-only: no copy on this computer at all.
      if (!rows.has(short)) {
        rows.set(short, {
          name: short,
          local: null,
          remote: remote.name,
          isCurrent: false,
          time: branch.time,
          behind: 0,
          ahead: 0,
          neverPushed: false,
          upstreamGone: false,
        })
      }
    }
  }

  return [...rows.values()]
}

/**
 * What may be done to a row.
 *
 * `losesWork` is the one that matters for a bulk delete. Deleting ten branches
 * at once has a much larger blast radius than deleting one, and the single-row
 * confirm only ever hedged with "may become hard to find".
 */
export function rowCapabilities(row: BranchRow): RowCapabilities {
  return {
    // Only a clean fast-forward: a branch with its own commits needs a real
    // merge, which is not something to do to ten branches without looking.
    canPull: !!row.local && row.behind > 0 && row.ahead === 0,
    // Never pushed anywhere, or holding commits the remote does not have.
    losesWork: !!row.local && (row.neverPushed || row.ahead > 0),
  }
}

/** Rows whose deletion would destroy the only copy of some work. */
export function riskyRows(rows: BranchRow[]): BranchRow[] {
  return rows.filter((row) => rowCapabilities(row).losesWork)
}

/** Matches a row against the search box, on name only. */
export function matchesQuery(row: BranchRow, query: string): boolean {
  const q = query.trim().toLowerCase()
  return q.length === 0 || row.name.toLowerCase().includes(q)
}

export type BranchSort = 'name' | 'stale'

/**
 * Sort rows for display. The current branch always leads: it cannot be deleted,
 * and burying it makes the list read as though it were missing.
 */
export function sortRows(rows: BranchRow[], sort: BranchSort): BranchRow[] {
  return [...rows].sort((a, b) => {
    if (a.isCurrent !== b.isCurrent) return a.isCurrent ? -1 : 1
    if (sort === 'stale') {
      // Oldest first: the point of this sort is finding what to clean up.
      const at = a.time ?? Number.MAX_SAFE_INTEGER
      const bt = b.time ?? Number.MAX_SAFE_INTEGER
      if (at !== bt) return at - bt
    }
    return a.name.localeCompare(b.name)
  })
}
