import { useMutation, useQueryClient, type QueryClient } from '@tanstack/react-query'
import { toast } from 'sonner'
import { commands, type Resolution, type SelectedLine } from '@/lib/bindings'
import { keys, unwrap } from '@/lib/queryKeys'

function invalidate(
  qc: QueryClient,
  repoId: string,
  which: Array<'status' | 'log' | 'branches' | 'stashes' | 'tags' | 'mergeState'>
) {
  for (const k of which) {
    qc.invalidateQueries({ queryKey: keys[k](repoId) })
  }
}

const onError = (e: Error) => toast.error(e.message)

export function useGitMutations(repoId: string | null) {
  const qc = useQueryClient()
  const id = repoId ?? ''

  const stageFile = useMutation({
    mutationFn: async (path: string) => unwrap(await commands.stageFile(id, path)),
    onSuccess: () => invalidate(qc, id, ['status']),
    onError,
  })

  const unstageFile = useMutation({
    mutationFn: async (path: string) => unwrap(await commands.unstageFile(id, path)),
    onSuccess: () => invalidate(qc, id, ['status']),
    onError,
  })

  const stageAll = useMutation({
    mutationFn: async () => unwrap(await commands.stageAll(id)),
    onSuccess: () => invalidate(qc, id, ['status']),
    onError,
  })

  const unstageAll = useMutation({
    mutationFn: async () => unwrap(await commands.unstageAll(id)),
    onSuccess: () => invalidate(qc, id, ['status']),
    onError,
  })

  const createCommit = useMutation({
    mutationFn: async (args: { summary: string; description: string }) =>
      unwrap(await commands.createCommit(id, args.summary, args.description)),
    onSuccess: (sha) => {
      invalidate(qc, id, ['status', 'log', 'branches'])
      toast(`Committed ${sha.slice(0, 7)}`)
    },
    onError,
  })

  const createBranch = useMutation({
    mutationFn: async (args: { name: string; checkout: boolean }) =>
      unwrap(await commands.createBranch(id, args.name, args.checkout)),
    onSuccess: (_d, args) => {
      invalidate(qc, id, ['branches', 'log'])
      toast(`Created branch ${args.name}`)
    },
    onError,
  })

  const checkout = useMutation({
    mutationFn: async (name: string) => unwrap(await commands.checkoutBranch(id, name)),
    onSuccess: (_d, name) => {
      invalidate(qc, id, ['status', 'log', 'branches'])
      toast(`Checked out ${name}`)
    },
    onError,
  })

  const stashSave = useMutation({
    mutationFn: async (message?: string) => unwrap(await commands.stashSave(id, message ?? null)),
    onSuccess: () => {
      invalidate(qc, id, ['status', 'stashes'])
      toast('Stashed changes')
    },
    onError,
  })

  const stashPop = useMutation({
    mutationFn: async (index: number) => unwrap(await commands.stashPop(id, index)),
    onSuccess: () => {
      invalidate(qc, id, ['status', 'stashes'])
      toast('Popped stash')
    },
    onError,
  })

  const fetch = useMutation({
    mutationFn: async () => unwrap(await commands.gitFetch(id)),
    onSuccess: () => {
      invalidate(qc, id, ['log', 'branches'])
      toast('Fetched all remotes')
    },
    onError,
  })

  const pull = useMutation({
    mutationFn: async () => unwrap(await commands.gitPull(id)),
    onSuccess: () => {
      invalidate(qc, id, ['status', 'log', 'branches'])
      toast('Pulled')
    },
    onError,
  })

  const push = useMutation({
    mutationFn: async () => unwrap(await commands.gitPush(id)),
    onSuccess: () => {
      invalidate(qc, id, ['log', 'branches'])
      toast('Pushed')
    },
    onError,
  })

  const merge = useMutation({
    mutationFn: async (reference: string) => ({
      reference,
      result: unwrap(await commands.mergeBranch(id, reference)),
    }),
    onSuccess: ({ reference, result }) => {
      invalidate(qc, id, ['status', 'log', 'branches', 'mergeState'])
      if (result.up_to_date) {
        toast(`Already up to date with ${reference}`)
      } else if (result.fast_forwarded) {
        toast(`Fast-forwarded to ${reference}`)
      } else if (result.conflicts.length > 0) {
        toast.warning(
          `Merged ${reference} with ${result.conflicts.length} conflict${result.conflicts.length === 1 ? '' : 's'} to resolve`
        )
      } else {
        toast(`Merged ${reference}`)
      }
    },
    onError,
  })

  const abortMerge = useMutation({
    mutationFn: async () => unwrap(await commands.abortMerge(id)),
    onSuccess: () => {
      invalidate(qc, id, ['status', 'log', 'branches', 'mergeState'])
      toast('Merge aborted')
    },
    onError,
  })

  const resolveConflict = useMutation({
    mutationFn: async (args: { path: string; resolution: Resolution }) =>
      unwrap(await commands.resolveConflict(id, args.path, args.resolution)),
    onSuccess: (_d, args) => {
      invalidate(qc, id, ['status', 'mergeState'])
      qc.invalidateQueries({ queryKey: keys.conflict(id, args.path) })
    },
    onError,
  })

  const commitMerge = useMutation({
    mutationFn: async (message: string) => unwrap(await commands.commitMerge(id, message)),
    onSuccess: (sha) => {
      invalidate(qc, id, ['status', 'log', 'branches', 'mergeState'])
      toast(`Merge committed ${sha.slice(0, 7)}`)
    },
    onError,
  })

  // Partial staging: refreshes status and every open file-diff view.
  const afterPatch = () => {
    invalidate(qc, id, ['status'])
    qc.invalidateQueries({ queryKey: ['diff', id] })
  }

  const stageLines = useMutation({
    mutationFn: async (args: { path: string; selection: SelectedLine[] }) =>
      unwrap(await commands.stageLines(id, args.path, args.selection)),
    onSuccess: afterPatch,
    onError,
  })

  const unstageLines = useMutation({
    mutationFn: async (args: { path: string; selection: SelectedLine[] }) =>
      unwrap(await commands.unstageLines(id, args.path, args.selection)),
    onSuccess: afterPatch,
    onError,
  })

  const discardLines = useMutation({
    mutationFn: async (args: { path: string; selection: SelectedLine[] }) =>
      unwrap(await commands.discardLines(id, args.path, args.selection)),
    onSuccess: () => {
      afterPatch()
      toast('Discarded selected lines')
    },
    onError,
  })

  return {
    stageFile,
    unstageFile,
    stageAll,
    unstageAll,
    createCommit,
    createBranch,
    checkout,
    stashSave,
    stashPop,
    fetch,
    pull,
    push,
    merge,
    abortMerge,
    resolveConflict,
    commitMerge,
    stageLines,
    unstageLines,
    discardLines,
  }
}
