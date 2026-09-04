import type { BranchInfo, RemoteInfo } from '@/lib/bindings'

export interface PublishedBranchTarget {
  remote: string
  branch: string
}

/**
 * Find the exact published branch linked to a local branch.
 *
 * A matching name on a remote is not enough: the local branch may track a
 * differently named branch, or the same name may exist on several remotes.
 * Only offer a remote delete after both the configured upstream and its
 * remote-tracking ref have been confirmed.
 */
export function publishedBranchTarget(
  local: BranchInfo | undefined,
  remotes: RemoteInfo[] | undefined,
): PublishedBranchTarget | null {
  if (!local?.upstream || !remotes) return null

  // Match known remote names instead of splitting at the first slash. Git
  // permits slashes in a remote name, and branch names commonly contain them.
  const remote = [...remotes]
    .sort((a, b) => b.name.length - a.name.length)
    .find((candidate) => local.upstream!.startsWith(`${candidate.name}/`))
  if (!remote) return null

  const branch = local.upstream.slice(remote.name.length + 1)
  if (!branch) return null

  const exists = remote.branches.some(
    (candidate) => candidate.name === branch || candidate.name === local.upstream,
  )
  return exists ? { remote: remote.name, branch } : null
}
