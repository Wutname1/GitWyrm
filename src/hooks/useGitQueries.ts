import { useInfiniteQuery, useQuery } from '@tanstack/react-query'
import { commands, type DiffSource } from '@/lib/bindings'
import { keys, unwrap } from '@/lib/queryKeys'
import { detectProvider, providerLabel } from '@/lib/remoteProvider'

const LOG_PAGE_SIZE = 200

export function useCommitLog(repoId: string | null) {
  return useInfiniteQuery({
    queryKey: keys.log(repoId ?? 'none'),
    enabled: repoId != null,
    initialPageParam: 0,
    queryFn: async ({ pageParam }) =>
      unwrap(await commands.getLog(repoId!, pageParam, LOG_PAGE_SIZE)),
    getNextPageParam: (lastPage, pages) =>
      lastPage.has_more ? pages.length * LOG_PAGE_SIZE : undefined,
  })
}

export function useStatus(repoId: string | null) {
  return useQuery({
    queryKey: keys.status(repoId ?? 'none'),
    enabled: repoId != null,
    queryFn: async () => unwrap(await commands.getStatus(repoId!)),
  })
}

export function useBranches(repoId: string | null) {
  return useQuery({
    queryKey: keys.branches(repoId ?? 'none'),
    enabled: repoId != null,
    queryFn: async () => unwrap(await commands.listBranches(repoId!)),
  })
}

export function useTags(repoId: string | null) {
  return useQuery({
    queryKey: keys.tags(repoId ?? 'none'),
    enabled: repoId != null,
    queryFn: async () => unwrap(await commands.listTags(repoId!)),
  })
}

export function useRemotes(repoId: string | null) {
  return useQuery({
    queryKey: keys.remotes(repoId ?? 'none'),
    enabled: repoId != null,
    queryFn: async () => unwrap(await commands.listRemotes(repoId!)),
  })
}

/**
 * The brand name of the host a branch pushes to -- "GitHub", "GitLab" -- so
 * menus can name the destination instead of saying "the remote".
 *
 * Falls back to the first remote for a branch with no upstream yet (the case
 * where the menu offers to publish it), and to null when the host is
 * self-hosted or the repo has no remotes at all.
 */
export function useBranchHost(repoId: string | null, upstream: string | null): string | null {
  const { data: remotes } = useRemotes(repoId)
  if (!remotes?.length) return null
  const name = upstream?.split('/')[0]
  const remote = (name && remotes.find((r) => r.name === name)) || remotes[0]
  return providerLabel(detectProvider(remote.url))
}

export function useStashes(repoId: string | null) {
  return useQuery({
    queryKey: keys.stashes(repoId ?? 'none'),
    enabled: repoId != null,
    queryFn: async () => unwrap(await commands.listStashes(repoId!)),
  })
}

export function useCommitDetail(repoId: string | null, sha: string | null) {
  return useQuery({
    queryKey: keys.commitDetail(repoId ?? 'none', sha ?? 'none'),
    enabled: repoId != null && sha != null,
    queryFn: async () => unwrap(await commands.getCommitDetail(repoId!, sha!)),
  })
}

export function useFileDiff(repoId: string | null, path: string | null, source: DiffSource | null) {
  return useQuery({
    queryKey: keys.fileDiff(repoId ?? 'none', path ?? 'none', source ?? { kind: 'unstaged' }),
    enabled: repoId != null && path != null && source != null,
    queryFn: async () => unwrap(await commands.getFileDiff(repoId!, path!, source!)),
  })
}

export function useMergeState(repoId: string | null) {
  return useQuery({
    queryKey: keys.mergeState(repoId ?? 'none'),
    enabled: repoId != null,
    queryFn: async () => unwrap(await commands.getMergeState(repoId!)),
  })
}

export function useConflict(repoId: string | null, path: string | null) {
  return useQuery({
    queryKey: keys.conflict(repoId ?? 'none', path ?? 'none'),
    enabled: repoId != null && path != null,
    queryFn: async () => unwrap(await commands.getConflict(repoId!, path!)),
  })
}
