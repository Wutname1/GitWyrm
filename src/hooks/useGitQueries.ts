import { useInfiniteQuery, useQuery } from '@tanstack/react-query'
import { commands, type DiffSource } from '@/lib/bindings'
import { keys, unwrap } from '@/lib/queryKeys'

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
