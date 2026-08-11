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
    // Derived from the last page's own param rather than `pages.length` so the
    // offset stays correct no matter how many pages the cache is holding.
    getNextPageParam: (lastPage, _pages, lastPageParam) =>
      lastPage.has_more ? lastPageParam + LOG_PAGE_SIZE : undefined,
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

/** Commits to send/get and uncommitted files for a single repo's tab badges. */
export interface RepoTabStatus {
  /** Commits ahead of upstream, waiting to push. */
  ahead: number
  /** Commits behind upstream, waiting to pull. */
  behind: number
  /** Staged plus unstaged files: the uncommitted-work count. */
  uncommitted: number
}

/**
 * The push/pull/uncommitted counts a repository tab shows at a glance.
 *
 * Asks the backend for exactly these three numbers rather than deriving them
 * from the full status and branch queries. Those carry per-file lists, changed
 * line counts and every branch's upstream, none of which a badge shows -- and
 * this runs for every open tab, on every external change, not just the one on
 * screen.
 *
 * It is invalidated by the same watcher event as the queries it replaced, so
 * the numbers are exactly as live as before; only the cost of producing them
 * changed.
 */
export function useRepoTabStatus(repoId: string | null): RepoTabStatus {
  const counts = useQuery({
    queryKey: keys.repoCounts(repoId ?? 'none'),
    enabled: repoId != null,
    queryFn: async () => unwrap(await commands.getRepoCounts(repoId!)),
  })

  return {
    ahead: counts.data?.ahead ?? 0,
    behind: counts.data?.behind ?? 0,
    uncommitted: counts.data?.uncommitted ?? 0,
  }
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

/**
 * Every submodule in the repo with its status, in sync or not. Drives the
 * sidebar list, so it includes healthy submodules -- "everything is fine" has
 * to be visible rather than inferred from an empty list.
 */
export function useSubmodules(repoId: string | null) {
  return useQuery({
    queryKey: keys.submodules(repoId ?? 'none'),
    enabled: repoId != null,
    queryFn: async () => unwrap(await commands.listSubmodules(repoId!)),
  })
}

/**
 * Every checkout of this project: the main folder first, then linked
 * worktrees.
 *
 * The section's own presence is driven by this query, so a worktree created in
 * a terminal makes the section appear without any other trigger -- the watcher
 * covers `.git/worktrees`, which is where that lands.
 */
export function useWorktrees(repoId: string | null) {
  return useQuery({
    queryKey: keys.worktrees(repoId ?? 'none'),
    enabled: repoId != null,
    queryFn: async () => unwrap(await commands.listWorktrees(repoId!)),
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
      return providerLabel(detectProvider(remote))
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

/** Commits that touched one file, newest first, following renames backwards. */
export function useFileHistory(repoId: string | null, path: string | null, limit = 200) {
  return useQuery({
    queryKey: keys.fileHistory(repoId ?? 'none', path ?? 'none'),
    enabled: repoId != null && path != null,
    queryFn: async () => unwrap(await commands.getFileHistory(repoId!, path!, limit)),
  })
}

/**
 * Line-by-line authorship. `sha` blames the file as of that commit; pass null
 * to blame the working copy.
 */
export function useFileBlame(repoId: string | null, path: string | null, sha: string | null = null) {
  return useQuery({
    queryKey: keys.fileBlame(repoId ?? 'none', path ?? 'none', sha),
    enabled: repoId != null && path != null,
    queryFn: async () => unwrap(await commands.getFileBlame(repoId!, path!, sha)),
  })
}

/**
 * The whole text of a file. `sha` reads it as of that commit; pass null to read
 * the working copy off disk.
 */
export function useFileContent(
  repoId: string | null,
  path: string | null,
  sha: string | null = null
) {
  return useQuery({
    queryKey: keys.fileContent(repoId ?? 'none', path ?? 'none', sha),
    enabled: repoId != null && path != null,
    queryFn: async () => unwrap(await commands.getFileContent(repoId!, path!, sha)),
  })
}

/**
 * True when a file has no commits behind it, so history and blame have nothing
 * to show. A file pinned to a commit is committed by definition; otherwise it
 * counts as new only if every entry the working tree has for it is an add -- a
 * file staged as added and then modified again still has no history.
 *
 * Returns false while status is loading: hiding actions and then popping them
 * back in reads worse than briefly offering one that opens an empty view.
 */
export function useNeverCommitted(repoId: string | null, path: string | null, sha: string | null) {
  const status = useStatus(repoId)
  if (sha != null || path == null || !status.data) return false
  const entries = [...status.data.staged, ...status.data.unstaged].filter((f) => f.path === path)
  return entries.length > 0 && entries.every((f) => f.status === 'A')
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
