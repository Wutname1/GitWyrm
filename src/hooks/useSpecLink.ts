import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { toast } from 'sonner'
import { commands } from '@/lib/bindings'
import { keys, unwrap } from '@/lib/queryKeys'

/**
 * The OpenSpec change a branch is working on, or null when it isn't linked.
 *
 * `explicit` distinguishes a link the user made from one read off the branch's
 * `Spec:` trailers, which the UI words differently -- an inferred link is a
 * guess, not a decision.
 */
export function useSpecLink(repoId: string | null, branch: string | null) {
  return useQuery({
    queryKey: keys.specLink(repoId ?? 'none', branch ?? 'none'),
    enabled: repoId != null && branch != null && branch !== 'HEAD',
    queryFn: async () => unwrap(await commands.specLinkGet(repoId!, branch!)),
  })
}

/** Link and unlink a branch to a change. */
export function useSpecLinkMutations(repoId: string | null) {
  const qc = useQueryClient()
  // Any link change can also change what the graph infers, so refresh every
  // branch's link rather than just the one that moved.
  const refresh = () => {
    if (repoId) qc.invalidateQueries({ queryKey: keys.specLinkAll(repoId) })
  }

  const link = useMutation({
    mutationFn: async (args: { branch: string; changeId: string }) =>
      unwrap(await commands.specLinkSet(repoId!, args.branch, args.changeId)),
    onSuccess: (_r, args) => {
      refresh()
      toast(`${args.branch} is working on ${args.changeId}`, {
        description: 'New commits on this branch will note the change.',
      })
    },
    onError: (e) => toast.error(String(e)),
  })

  const unlink = useMutation({
    mutationFn: async (args: { branch: string }) =>
      unwrap(await commands.specLinkClear(repoId!, args.branch)),
    onSuccess: (_r, args) => {
      refresh()
      toast(`${args.branch} is no longer tied to a change`, {
        description: 'Commits you already made keep their labels.',
      })
    },
    onError: (e) => toast.error(String(e)),
  })

  return { link, unlink }
}
