import type { DiffSource } from './bindings'

export const keys = {
  log: (repoId: string) => ['log', repoId] as const,
  status: (repoId: string) => ['status', repoId] as const,
  branches: (repoId: string) => ['branches', repoId] as const,
  tags: (repoId: string) => ['tags', repoId] as const,
  remoteTags: (repoId: string, remote: string) => ['remoteTags', repoId, remote] as const,
  remotes: (repoId: string) => ['remotes', repoId] as const,
  stashes: (repoId: string) => ['stashes', repoId] as const,
  commitDetail: (repoId: string, sha: string) => ['commit', repoId, sha] as const,
  fileDiff: (repoId: string, path: string, source: DiffSource) =>
    ['diff', repoId, path, source] as const,
  fileHistory: (repoId: string, path: string) => ['fileHistory', repoId, path] as const,
  fileBlame: (repoId: string, path: string, sha: string | null) =>
    ['fileBlame', repoId, path, sha] as const,
  mergeState: (repoId: string) => ['mergeState', repoId] as const,
  conflict: (repoId: string, path: string) => ['conflict', repoId, path] as const,
}

/** Unwraps tauri-specta's Result<T, string> into T-or-throw for TanStack Query. */
export function unwrap<T>(result: { status: 'ok'; data: T } | { status: 'error'; error: string }): T {
  if (result.status === 'error') throw new Error(result.error)
  return result.data
}
