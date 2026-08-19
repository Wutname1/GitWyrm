import { useMutation, useQueries, useQuery } from '@tanstack/react-query'
import { useMemo } from 'react'
import { toast } from 'sonner'
import { commands, type RepoInfo, type ScannedRepo } from '@/lib/bindings'
import { normalizePath, pathKey, pathName } from '@/lib/paths'
import { unwrap } from '@/lib/queryKeys'
import { timed } from '@/lib/perfTrail'
import { Sentry } from '@/lib/sentry'
import { useUiStore } from '@/stores/uiStore'
import { primaryFirst, useWorkspaceStore, type CodeFolder } from '@/stores/workspaceStore'

/** One watched folder's scan result. */
export interface CodeFolderScan extends CodeFolder {
  repos: ScannedRepo[]
  isLoading: boolean
  /** True when the folder could not be read -- usually a drive that is not plugged in. */
  isUnavailable: boolean
}

export interface CodeFolderRepos {
  /** Per folder, primary first, so a failing drive is visible on its own row. */
  folders: CodeFolderScan[]
  /** Every repo across every folder, de-duplicated by path. */
  repos: ScannedRepo[]
  isLoading: boolean
  isFetching: boolean
  /** Rescan every watched folder. */
  refetch: () => void
}

/**
 * Scanned repos across every watched folder (no repo handles taken).
 *
 * Each folder is its own query so one slow or disconnected drive never blocks
 * the others -- a folder that fails shows as unavailable while the rest list
 * normally.
 */
export function useCodeFolderRepos(): CodeFolderRepos {
  const codeFolders = useWorkspaceStore((s) => s.codeFolders)
  const ordered = useMemo(() => primaryFirst(codeFolders), [codeFolders])

  const results = useQueries({
    queries: ordered.map((folder) => ({
      queryKey: ['code-folder', pathKey(folder.path)],
      staleTime: 60_000,
      // A missing folder is an expected state (drive unplugged), not a fault to
      // retry: fail it once and let the row say so.
      retry: false,
      queryFn: async () => unwrap(await commands.scanCodeFolder(folder.path)),
    })),
  })

  return useMemo(() => {
    const folders = ordered.map((folder, index) => {
      const result = results[index]
      return {
        ...folder,
        repos: result?.data ?? [],
        isLoading: result?.isLoading ?? false,
        isUnavailable: result?.isError ?? false,
      }
    })

    // The same repo can sit under two overlapping folders; show it once.
    const byPath = new Map<string, ScannedRepo>()
    for (const folder of folders) {
      for (const repo of folder.repos) {
        const key = pathKey(repo.path)
        if (!byPath.has(key)) byPath.set(key, repo)
      }
    }

    return {
      folders,
      repos: [...byPath.values()],
      isLoading: results.some((result) => result.isLoading),
      isFetching: results.some((result) => result.isFetching),
      refetch: () => {
        for (const result of results) void result.refetch()
      },
    }
  }, [ordered, results])
}

/**
 * A repository's readme text, or null when it has none.
 *
 * Reads the file straight off disk without opening the repository, so this is
 * safe for any row in the library list, including ones never opened.
 */
export function useRepoReadme(path: string | null) {
  return useQuery({
    queryKey: ['repo-readme', path ? pathKey(path) : ''],
    enabled: path != null,
    staleTime: 60_000,
    retry: false,
    queryFn: async () => unwrap(await commands.readRepoReadme(path as string)),
  })
}

/**
 * Icons for repositories GitWyrm has opened before, keyed by normalized path.
 *
 * Reads only what discovery already recorded, so listing a large library costs
 * one file read per repository that has a known icon and nothing at all for the
 * rest. Repositories never opened, and ones whose icon file has since been
 * deleted, simply have no entry and fall back to the folder glyph.
 */
export function useCachedRepoIcons(paths: string[]) {
  // Sorted so the key is stable however the caller ordered the list, and the
  // scan-driven library list re-ordering itself does not refetch.
  const sorted = [...new Set(paths.map(normalizePath))].sort()
  const query = useQuery({
    queryKey: ['cached-repo-icons', sorted],
    enabled: sorted.length > 0,
    staleTime: 60_000,
    queryFn: async () => unwrap(await commands.getCachedRepoIcons(sorted)),
  })

  return useMemo(() => {
    const byPath = new Map<string, string>()
    for (const icon of query.data ?? []) {
      byPath.set(pathKey(icon.repo_path), icon.data_url)
    }
    return byPath
  }, [query.data])
}

/**
 * Record whether a repository's folder is still on disk, so settings for a
 * folder that has gone for good are eventually forgotten while a folder that
 * comes back - a delete and re-clone, a reconnected drive - keeps everything.
 *
 * Only a genuinely absent folder counts as missing. A folder that is present
 * but fails to open for some other reason (a broken repository, a permissions
 * problem) keeps its settings, because the user has not thrown it away.
 */
export async function noteRepoAvailability(path: string, opened: boolean) {
  try {
    const missing = opened ? false : !(await commands.pathExists(path))
    // Nothing to record for a present-but-unopenable folder.
    if (!opened && !missing) return
    await commands.markRepoMissing(path, missing)
  } catch {
    // Bookkeeping only: never turn this into a visible failure.
  }
}

/** Opens a repo with loading feedback (toast while the backend works). */
export function useOpenRepo() {
  const addRepo = useWorkspaceStore((s) => s.addRepo)
  const closeModal = useUiStore((s) => s.closeModal)
  const closeRepoPicker = useUiStore((s) => s.closeRepoPicker)

  return useMutation({
    mutationFn: async (rawPath: string) => {
      const path = normalizePath(rawPath)
      const name = pathName(path)
      const toastId = toast.loading(`Opening ${name}…`)
      try {
        // Spans the whole IPC round-trip so the felt open latency (dominated by
        // the backend discover + head phases) lands in the Sentry dashboard.
        // Also recorded to the in-memory perf trail, so a bug report about a
        // slow open carries the actual number. The Sentry span alone cannot:
        // it lands in a separate dataset with nothing tying it to the report.
        return await timed('openRepo', () =>
          Sentry.startSpan({ name: 'openRepo', op: 'git.open' }, () =>
            commands.openRepo(path).then(unwrap)
          )
        )
      } finally {
        toast.dismiss(toastId)
      }
    },
    onSuccess: (repo) => {
      // The picker did its job, so its tab retires rather than sitting there
      // next to the repository it just opened.
      closeRepoPicker()
      addRepo(repo)
      closeModal()
      void noteRepoAvailability(repo.path, true)
      toast.success(`Opened ${repo.name}`)
    },
    onError: (e: Error, rawPath) => {
      void noteRepoAvailability(normalizePath(rawPath), false)
      toast.error(e.message)
    },
  })
}

/**
 * Opens a submodule as a tab attached to the project it belongs to, rather than
 * as a project of its own. The tab nests under its parent and travels with it,
 * because that is what the folder actually is: a part of the project above it.
 */
export function useOpenSubmoduleRepo() {
  const addSubmoduleRepo = useWorkspaceStore((s) => s.addSubmoduleRepo)
  const closeModal = useUiStore((s) => s.closeModal)
  const closeRepoPicker = useUiStore((s) => s.closeRepoPicker)

  return useMutation({
    mutationFn: async ({ path: rawPath }: { path: string; parentPath: string }) => {
      const path = normalizePath(rawPath)
      const name = pathName(path)
      const toastId = toast.loading(`Opening ${name}…`)
      try {
        // Also recorded to the in-memory perf trail, so a bug report about a
        // slow open carries the actual number. The Sentry span alone cannot:
        // it lands in a separate dataset with nothing tying it to the report.
        return await timed('openRepo', () =>
          Sentry.startSpan({ name: 'openRepo', op: 'git.open' }, () =>
            commands.openRepo(path).then(unwrap)
          )
        )
      } finally {
        toast.dismiss(toastId)
      }
    },
    onSuccess: (repo, { parentPath }) => {
      closeRepoPicker()
      addSubmoduleRepo(repo, parentPath)
      closeModal()
      void noteRepoAvailability(repo.path, true)
      toast.success(`Opened ${repo.name}`)
    },
    onError: (e: Error, { path }) => {
      void noteRepoAvailability(normalizePath(path), false)
      toast.error(e.message)
    },
  })
}

/**
 * Opens several repos as tabs in one go. Failures are collected rather than
 * aborting the batch, so one unreadable folder cannot block the rest.
 */
export function useOpenRepos() {
  const addReposInBackground = useWorkspaceStore((s) => s.addReposInBackground)
  const closeModal = useUiStore((s) => s.closeModal)
  const closeRepoPicker = useUiStore((s) => s.closeRepoPicker)

  return useMutation({
    mutationFn: async (rawPaths: string[]) => {
      const toastId = toast.loading(
        `Opening ${rawPaths.length} ${rawPaths.length === 1 ? 'repository' : 'repositories'}…`
      )
      try {
        const opened: RepoInfo[] = []
        const failed: string[] = []
        for (const rawPath of rawPaths) {
          const path = normalizePath(rawPath)
          try {
            opened.push(unwrap(await commands.openRepo(path)))
            void noteRepoAvailability(path, true)
          } catch {
            void noteRepoAvailability(path, false)
            failed.push(pathName(path))
          }
        }
        return { opened, failed }
      } finally {
        toast.dismiss(toastId)
      }
    },
    onSuccess: ({ opened, failed }) => {
      addReposInBackground(opened)
      if (opened.length > 0) {
        closeRepoPicker()
        closeModal()
      }
      if (failed.length === 0) {
        toast.success(`Opened ${opened.length} ${opened.length === 1 ? 'repository' : 'repositories'}`)
      } else if (opened.length > 0) {
        toast.warning(`Opened ${opened.length}, but couldn't open ${failed.join(', ')}`)
      } else {
        toast.error(`Couldn't open ${failed.join(', ')}`)
      }
    },
    onError: (e: Error) => toast.error(e.message),
  })
}
