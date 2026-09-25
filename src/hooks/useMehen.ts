import { type QueryClient, useQuery } from '@tanstack/react-query'
import { toast } from 'sonner'
import { commands, type BranchInfo, type MehenPushNote, type MehenRepoStatus } from '@/lib/bindings'
import { keys, unwrap } from '@/lib/queryKeys'
import { classifyError } from '@/lib/errorClass'
import { showErrorToast } from '@/lib/errorToast'
import { pushNoteToast } from '@/lib/mehen'
import { samePath } from '@/lib/paths'

/**
 * What Mehen last found in every repository it checks, or null when Mehen has
 * never written its summary. One read serves every tab. Re-read when the
 * window comes back into focus: the usual way the answer changes is the user
 * switching to Mehen, fixing something, and coming back.
 */
export function useMehenOverview() {
  return useQuery({
    queryKey: keys.mehenOverview(),
    queryFn: async () => unwrap(await commands.mehenOverview()),
    refetchOnWindowFocus: true,
    staleTime: 30_000,
  })
}

/** One repository's entry, or null when Mehen has never checked it. */
export function findMehenStatus(repos: MehenRepoStatus[] | undefined, path: string | null | undefined) {
  if (!repos || !path) return null
  return repos.find((r) => samePath(r.path, path)) ?? null
}

export function useMehenStatus(repoPath: string | null) {
  const { data } = useMehenOverview()
  return { status: findMehenStatus(data?.repos, repoPath), canOpen: data?.can_open ?? false }
}

/**
 * A heads-up for pushing the checked-out branch, or null when there is
 * nothing to say. Only asked for when there is something to send.
 */
export function useMehenPushNote(repoId: string | null, head: BranchInfo | undefined) {
  const ahead = head?.ahead ?? 0
  const unpublished = head?.sync.kind === 'never_pushed'
  return useQuery({
    queryKey: keys.mehenPushNote(repoId ?? 'none', head?.name ?? null, head?.tip ?? null, ahead),
    enabled: repoId != null && head != null && (ahead > 0 || unpublished),
    queryFn: async () => unwrap(await commands.mehenPushNote(repoId!)),
    refetchOnWindowFocus: true,
    staleTime: 30_000,
  })
}

/**
 * The push note already worked out for the branch about to be pushed, read
 * from the cache so pushing never waits on it. The newest entry is the one for
 * the branch as it is now.
 */
export function cachedPushNote(qc: QueryClient, repoId: string): MehenPushNote | null {
  const [latest] = qc
    .getQueryCache()
    .findAll({ queryKey: keys.mehenPushNoteAll(repoId) })
    .filter((q) => q.state.data != null)
    .sort((a, b) => b.state.dataUpdatedAt - a.state.dataUpdatedAt)
  return (latest?.state.data as MehenPushNote | undefined) ?? null
}

/** Shown once a push that changed dependency files has gone through. */
export function showPushNoteToast(repoId: string, note: MehenPushNote) {
  const { title, description } = pushNoteToast(note)
  toast.info(title, {
    description,
    duration: 10000,
    action: note.can_open ? { label: 'Fix in Mehen', onClick: () => void openInMehen(repoId, true) } : undefined,
  })
}

/** With `fix`, Mehen opens with this repository's security fixes already selected. */
export async function openInMehen(repoId: string, fix: boolean) {
  try {
    unwrap(await commands.openInMehen(repoId, fix))
    toast(fix ? 'Opening Mehen with the fixes selected' : 'Opening this project in Mehen')
  } catch (error) {
    showErrorToast(classifyError(error))
  }
}
