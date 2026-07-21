import { useCallback } from 'react'
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

/**
 * Tags the remote already has. This is a network call, so it is not part of
 * `useTags` and does not refetch on focus -- the sidebar only needs it fresh
 * enough to tell local-only tags apart, and a stale answer is better than a
 * lookup on every window switch. Pass an empty remote to use the default one.
 *
 * While it is loading or after it fails, callers get no data at all rather
 * than an empty list, so a tag is never mislabelled "not sent" just because
 * we are offline or have not checked yet.
 */
export function useRemoteTags(repoId: string | null, remote = '', enabled = true) {
  return useQuery({
    queryKey: keys.remoteTags(repoId ?? 'none', remote),
    enabled: repoId != null && enabled,
    queryFn: async () => unwrap(await commands.listRemoteTags(repoId!, remote)),
    staleTime: 5 * 60 * 1000,
    refetchOnWindowFocus: false,
    retry: false,
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
 * Resolves an upstream ref ("origin/main") to the brand name of the host it
 * lives on -- "GitHub", "GitLab" -- so the UI can name the destination
 * instead of saying "the remote".
 *
 * Returns a function rather than a value because push and pull only learn
 * which upstream they touched from the result, after the call.
 *
 * Falls back to the first remote when the upstream is unknown (a branch not
 * yet published), and to null for self-hosted or unrecognized hosts and for
 * repos with no remotes at all.
 */
export function useHostResolver(repoId: string | null): (upstream: string | null) => string | null {
  const { data: remotes } = useRemotes(repoId)
  return useCallback(
    (upstream: string | null) => {
      if (!remotes?.length) return null
      const name = upstream?.split('/')[0]
      const remote = (name && remotes.find((r) => r.name === name)) || remotes[0]
      return providerLabel(detectProvider(remote.url))
    },
    [remotes]
  )
}

/** The host a single branch pushes to. See [`useHostResolver`]. */
export function useBranchHost(repoId: string | null, upstream: string | null): string | null {
  return useHostResolver(repoId)(upstream)
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
