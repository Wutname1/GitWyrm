import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { commands, type SpecChange, type SpecTask } from '@/lib/bindings'
import { invalidateOpenspec, keys, unwrap } from '@/lib/queryKeys'
import { useUiStore } from '@/stores/uiStore'

/**
 * Whether this repository uses OpenSpec, plus the change counts and CLI state.
 *
 * Every Specs surface gates on `present`: a repo without an `openspec/` folder
 * shows no sidebar section, no spec card, and no status-bar segment at all.
 */
export function useOpenspecStatus(repoId: string | null) {
  return useQuery({
    queryKey: keys.openspecStatus(repoId ?? 'none'),
    enabled: repoId != null,
    queryFn: async () => unwrap(await commands.openspecStatus(repoId!)),
  })
}

/**
 * Active changes, newest first. Empty (not an error) for a repo with no
 * `openspec/` folder, so callers can render from `data ?? []` unconditionally.
 */
export function useOpenspecChanges(repoId: string | null) {
  return useQuery({
    queryKey: keys.openspecChanges(repoId ?? 'none'),
    enabled: repoId != null,
    queryFn: async () => unwrap(await commands.openspecListChanges(repoId!)),
  })
}

/**
 * Commits that touched a change's folder, newest first.
 *
 * Only fetched when something is actually showing history, since it shells out to
 * git log per change.
 */
export function useOpenspecHistory(
  repoId: string | null,
  changeId: string | null,
  enabled = true
) {
  return useQuery({
    queryKey: keys.openspecHistory(repoId ?? 'none', changeId ?? 'none'),
    enabled: repoId != null && changeId != null && enabled,
    queryFn: async () => unwrap(await commands.openspecChangeHistory(repoId!, changeId!)),
  })
}

/** The built-in archive-commit message template, for the settings placeholder and reset. */
export function useOpenspecDefaultArchiveTemplate() {
  return useQuery({
    queryKey: ['openspec-default-archive-template'],
    staleTime: Infinity,
    queryFn: async () => await commands.openspecDefaultArchiveCommitTemplate(),
  })
}

/** Ids of archived changes, newest first. Only fetched when asked for. */
export function useOpenspecArchived(repoId: string | null, enabled = false) {
  return useQuery({
    queryKey: keys.openspecArchived(repoId ?? 'none'),
    enabled: repoId != null && enabled,
    queryFn: async () => unwrap(await commands.openspecArchivedIds(repoId!)),
  })
}

/**
 * The change every Specs surface is currently pointed at.
 *
 * `null` means nothing is selected, and stays that way -- a surface that
 * resolved null to the first change would make the first row impossible to
 * click away from, since deselecting it would immediately reselect it.
 *
 * Pass `fallbackToFirst` for surfaces that must always show something (the spec
 * card). That also covers a selected change going away -- archived, renamed, or
 * deleted by another tool -- so the card does not blank out when the folder
 * changes underneath it.
 */
export function useSelectedChange(repoId: string | null, fallbackToFirst = false) {
  const changes = useOpenspecChanges(repoId)
  const selectedId = useUiStore((s) => s.selectedChangeId)
  const list = changes.data ?? []
  const match = list.find((c) => c.id === selectedId)
  const selected = match ?? (fallbackToFirst ? list[0] : undefined)
  return { change: selected, changes: list, query: changes }
}

/** Progress across every active change, for a header count. */
export function totalProgress(changes: SpecChange[]) {
  return changes.reduce(
    (acc, c) => ({ done: acc.done + c.progress.done, total: acc.total + c.progress.total }),
    { done: 0, total: 0 }
  )
}

/** The first task not yet done -- what a handoff or run targets. */
export function nextTask(change: SpecChange): SpecTask | undefined {
  return change.tasks.find((t) => !t.done)
}

/**
 * Writes to a repo's openspec files.
 *
 * Each mutation refreshes the spec queries on settle rather than optimistically
 * patching the cache. The files are the state and other tools write them too, so
 * the honest thing to show is what the parser reads back -- and a re-read is
 * cheap (a handful of small markdown files).
 */
export function useOpenspecMutations(repoId: string | null) {
  const qc = useQueryClient()
  const refresh = () => {
    if (repoId) invalidateOpenspec(qc, repoId)
  }

  const toggleTask = useMutation({
    mutationFn: async (vars: { changeId: string; line: number; done: boolean }) =>
      unwrap(await commands.openspecToggleTask(repoId!, vars.changeId, vars.line, vars.done)),
    onSettled: refresh,
  })

  const scaffoldChange = useMutation({
    mutationFn: async (vars: { name: string; description: string }) =>
      unwrap(await commands.openspecScaffoldChange(repoId!, vars.name, vars.description)),
    onSettled: refresh,
  })

  const validateChange = useMutation({
    mutationFn: async (changeId: string) =>
      unwrap(await commands.openspecValidateChange(repoId!, changeId)),
    // Validation reads; nothing to refresh.
  })

  const archiveChange = useMutation({
    mutationFn: async (changeId: string) =>
      unwrap(await commands.openspecArchiveChange(repoId!, changeId)),
    onSettled: refresh,
  })

  return { toggleTask, scaffoldChange, validateChange, archiveChange }
}
