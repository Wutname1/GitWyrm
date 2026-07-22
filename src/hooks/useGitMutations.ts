import { useMutation, useQueryClient, type QueryClient } from '@tanstack/react-query'
import { toast } from 'sonner'
import {
  commands,
  type PullResult,
  type PushResult,
  type Resolution,
  type ResetMode,
  type SelectedLine,
} from '@/lib/bindings'
import { keys, unwrap } from '@/lib/queryKeys'
import { useHostResolver } from '@/hooks/useGitQueries'
import { classifyError } from '@/lib/errorClass'
import { copyToClipboard } from '@/lib/clipboard'
import { plural, shortSha } from '@/lib/gitDisplay'
import { log } from '@/lib/log'
import { useWorkspaceStore } from '@/stores/workspaceStore'
import { useUiStore } from '@/stores/uiStore'

type QueryName = 'status' | 'log' | 'branches' | 'stashes' | 'tags' | 'remotes' | 'mergeState'

interface FolderFilesArgs {
  folder: string
  paths: string[]
}

/**
 * Named sets for the invalidations that are easy to get wrong.
 *
 * The graph draws its ref pills from the log query (`collect_refs` in
 * commands/log.rs), not from the branch list -- so anything that adds,
 * removes or renames a ref has to refresh `log` too, or the pill lingers
 * in the graph after the branch is gone.
 */
const REFS: QueryName[] = ['branches', 'log']
const REMOTE_REFS: QueryName[] = ['remotes', 'branches', 'log']

function invalidate(qc: QueryClient, repoId: string, which: QueryName[]) {
  for (const k of which) {
    qc.invalidateQueries({ queryKey: keys[k](repoId) })
  }
}

/**
 * Every mutation failure flows through here. It logs the raw error to
 * gitwyrm.log (so there is always a durable trace) and shows the user a
 * classified, plain-language toast at the right severity -- info for benign
 * no-ops, warning for recoverable conflicts, error for real failures.
 */
const onError = (e: Error) => {
  const { severity, message, raw } = classifyError(e)
  log[severity === 'info' ? 'info' : severity === 'warning' ? 'warn' : 'error'](
    `mutation failed [${severity}]: ${raw}`
  )
  if (severity === 'info') toast.info(message)
  else if (severity === 'warning') toast.warning(message)
  else toast.error(message)
}

const commitCount = (n: number) => plural(n, 'commit')

/**
 * Where the commits went, named the way the user thinks of it: the upstream
 * branch plus the host it lives on ("origin/main on GitHub"). `host` is null
 * for self-hosted or unrecognized remotes, which drop back to the branch name
 * alone, and to a generic stand-in when even that is unknown.
 */
const describeTarget = (r: { upstream: string | null }, host: string | null) => {
  if (!r.upstream) return host ?? 'the remote'
  return host ? `${r.upstream} on ${host}` : r.upstream
}

/**
 * Push and pull report what actually moved, measured from the branch's
 * ahead/behind before and after. A no-op says so plainly rather than claiming
 * work happened -- the user asked for the truth of what the operation did.
 */
function describePush(r: PushResult, host: string | null): string {
  if (r.pushed === 0) {
    return r.branch
      ? `Nothing to send - ${describeTarget(r, host)} already matches ${r.branch}`
      : `Nothing to send - ${host ?? 'the remote'} is already up to date`
  }
  return `Sent ${commitCount(r.pushed)} to ${describeTarget(r, host)}`
}

function describePull(r: PullResult, host: string | null): string {
  if (r.received === 0) {
    const base = r.branch
      ? `Nothing new to get - ${r.branch} already matches ${describeTarget(r, host)}`
      : 'Nothing new to get - you are already up to date'
    // Checked for incoming work but still have outgoing work of our own.
    return r.ahead_after > 0
      ? `${base}. You still have ${commitCount(r.ahead_after)} to send.`
      : base
  }
  const base = `Got ${commitCount(r.received)} from ${describeTarget(r, host)}`
  return r.ahead_after > 0
    ? `${base}. You still have ${commitCount(r.ahead_after)} to send.`
    : base
}

export function useGitMutations(repoId: string | null) {
  const qc = useQueryClient()
  const id = repoId ?? ''
  const hostOf = useHostResolver(repoId)
  const tagPushDefault = useWorkspaceStore((s) => s.tagPushDefault)
  const promptPushTags = useUiStore((s) => s.promptPushTags)

  /**
   * After a push, deal with tags the remote still lacks.
   *
   * Every local-only tag is offered, including ones pointing at commits that
   * were never pushed -- git sends those commits along with the tag, so the
   * push does work. The prompt flags them so that isn't invisible.
   *
   * The remembered choice decides whether we send silently, stay quiet, or ask.
   * Runs after the push has already reported success, so a failure here is
   * logged and dropped rather than turning a good push into an error.
   */
  const handleTagsAfterPush = async () => {
    if (tagPushDefault === 'never') return
    try {
      const pending = unwrap(await commands.unpushedTags(id, ''))
      if (pending.length === 0) return

      if (tagPushDefault === 'always') {
        for (const tag of pending) {
          await unwrap(await commands.pushTag(id, tag.name, ''))
        }
        qc.invalidateQueries({ queryKey: ['remoteTags', id] })
        toast(`Sent ${plural(pending.length, 'tag')}`)
        return
      }
      promptPushTags(pending.map((t) => ({ name: t.name, carriesCommits: !t.commit_on_remote })))
    } catch (e) {
      log.warn(`could not check for unsent tags: ${String(e)}`)
    }
  }

  const stageFile = useMutation({
    mutationFn: async (path: string) => unwrap(await commands.stageFile(id, path)),
    onSuccess: () => invalidate(qc, id, ['status']),
    onError,
  })

  const stageFiles = useMutation({
    mutationFn: async ({ paths }: FolderFilesArgs) => unwrap(await commands.stageFiles(id, paths)),
    onSuccess: (_data, { folder, paths }) => {
      invalidate(qc, id, ['status'])
      toast(`Staged ${plural(paths.length, 'file')} in ${folder}`)
    },
    onError,
  })

  const unstageFile = useMutation({
    mutationFn: async (path: string) => unwrap(await commands.unstageFile(id, path)),
    onSuccess: () => invalidate(qc, id, ['status']),
    onError,
  })

  const unstageFiles = useMutation({
    mutationFn: async ({ paths }: FolderFilesArgs) => unwrap(await commands.unstageFiles(id, paths)),
    onSuccess: (_data, { folder, paths }) => {
      invalidate(qc, id, ['status'])
      toast(`Unstaged ${plural(paths.length, 'file')} in ${folder}`)
    },
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

  const discardFile = useMutation({
    mutationFn: async (path: string) => {
      await unwrap(await commands.discardFile(id, path))
      return path
    },
    onSuccess: (path) => {
      invalidate(qc, id, ['status'])
      toast(`Discarded changes in ${path.split('/').pop()}`)
    },
    onError,
  })

  const discardFiles = useMutation({
    mutationFn: async ({ paths }: FolderFilesArgs) => unwrap(await commands.discardFiles(id, paths)),
    onSuccess: (_data, { folder, paths }) => {
      invalidate(qc, id, ['status'])
      toast(`Discarded ${plural(paths.length, 'file')} in ${folder}`)
    },
    onError,
  })

  // Adding a line to .gitignore changes the working tree, so the status list
  // has to refresh for the newly ignored file to disappear from it.
  const addToGitignore = useMutation({
    mutationFn: async (args: { pattern: string; label: string }) => ({
      outcome: unwrap(await commands.addToGitignore(id, args.pattern)),
      ...args,
    }),
    onSuccess: ({ outcome, pattern, label }) => {
      invalidate(qc, id, ['status'])
      if (outcome === 'already_present') toast.info(`${pattern} was already ignored`)
      else toast(`Now ignoring ${label}`)
    },
    onError,
  })

  const discardAll = useMutation({
    mutationFn: async () => unwrap(await commands.discardAll(id)),
    onSuccess: () => {
      invalidate(qc, id, ['status'])
      toast('Discarded all changes')
    },
    onError,
  })

  // Snap a submodule back to the commit the parent repo records, or download it
  // for the first time when `init` is set.
  const updateSubmodule = useMutation({
    mutationFn: async (args: { path: string; init: boolean }) => {
      await unwrap(await commands.updateSubmodule(id, args.path, args.init))
      return args
    },
    onSuccess: ({ path, init }) => {
      invalidate(qc, id, ['status', 'log'])
      const name = path.split('/').pop() ?? path
      toast(init ? `Downloaded ${name}` : `Reset ${name} to the recorded commit`)
    },
    onError,
  })

  const createCommit = useMutation({
    mutationFn: async (args: { summary: string; description: string }) =>
      unwrap(await commands.createCommit(id, args.summary, args.description)),
    onSuccess: (sha) => {
      invalidate(qc, id, ['status', 'log', 'branches'])
      toast(`Committed ${shortSha(sha)}`)
    },
    onError,
  })

  const createBranch = useMutation({
    mutationFn: async (args: { name: string; sha?: string; checkout: boolean }) =>
      unwrap(await commands.createBranch(id, args.name, args.sha ?? '', args.checkout)),
    onSuccess: (_d, args) => {
      invalidate(qc, id, REFS)
      toast(`Created branch ${args.name}`)
    },
    onError,
  })

  const deleteBranch = useMutation({
    mutationFn: async (name: string) => {
      await unwrap(await commands.deleteBranch(id, name))
      return name
    },
    onSuccess: (name) => {
      invalidate(qc, id, REFS)
      toast(`Deleted branch ${name}`)
    },
    onError,
  })

  const createTag = useMutation({
    mutationFn: async (args: { name: string; sha: string; message: string; push?: boolean }) => {
      await unwrap(await commands.createTag(id, args.name, args.sha, args.message))
      // Push inside the same mutation so the two never drift apart in the UI:
      // one action, one toast, one settled state.
      if (args.push) {
        await unwrap(await commands.pushTag(id, args.name, ''))
      }
      return args
    },
    onSuccess: (args) => {
      invalidate(qc, id, ['tags', 'log'])
      if (args.push) qc.invalidateQueries({ queryKey: ['remoteTags', id] })
      toast(args.push ? `Created and sent tag ${args.name}` : `Created tag ${args.name}`)
    },
    onError,
  })

  const deleteTag = useMutation({
    mutationFn: async (name: string) => {
      await unwrap(await commands.deleteTag(id, name))
      return name
    },
    onSuccess: (name) => {
      invalidate(qc, id, ['tags', 'log'])
      toast(`Deleted tag ${name}`)
    },
    onError,
  })

  /** Send one tag to a remote. Empty `remote` targets the default one. */
  const pushTag = useMutation({
    mutationFn: async (args: { name: string; remote?: string }) => {
      await unwrap(await commands.pushTag(id, args.name, args.remote ?? ''))
      return args.name
    },
    onSuccess: (name) => {
      invalidate(qc, id, ['tags'])
      qc.invalidateQueries({ queryKey: ['remoteTags', id] })
      toast(`Sent tag ${name}`)
    },
    onError,
  })

  /**
   * Remove a tag from the remote. The local copy is untouched, so a tag can be
   * un-published without losing it here.
   */
  const deleteRemoteTag = useMutation({
    mutationFn: async (args: { name: string; remote?: string }) => {
      await unwrap(await commands.deleteRemoteTag(id, args.name, args.remote ?? ''))
      return args.name
    },
    onSuccess: (name) => {
      qc.invalidateQueries({ queryKey: ['remoteTags', id] })
      toast(`Removed tag ${name} from the remote`)
    },
    onError,
  })

  // Hand the repo off to an external program. No cache to touch.
  const revealInFileManager = useMutation({
    mutationFn: async () => unwrap(await commands.revealInFileManager(id)),
    onError,
  })

  const openInEditor = useMutation({
    mutationFn: async () => unwrap(await commands.openInEditor(id)),
    onError,
  })

  const openInTerminal = useMutation({
    mutationFn: async () => unwrap(await commands.openInTerminal(id)),
    onError,
  })

  const checkout = useMutation({
    mutationFn: async (name: string) => {
      const mode = useWorkspaceStore.getState().branchSwitchMode
      return { name, outcome: unwrap(await commands.checkoutBranch(id, name, mode)) }
    },
    onSuccess: ({ name, outcome }) => {
      invalidate(qc, id, ['status', 'log', 'branches', 'stashes'])
      if (outcome === 'stash_pop_conflict') {
        toast.warning(
          `Switched to ${name}, but your changes conflict — resolve the markers. Your stash was kept as a backup.`
        )
      } else if (outcome === 'stashed') {
        toast(`Checked out ${name} with your changes`)
      } else {
        toast(`Checked out ${name}`)
      }
    },
    onError,
  })

  // Stash rows render in the graph, so every stash mutation refreshes `log`
  // too -- otherwise a popped stash lingers as a ghost row.
  const stashSave = useMutation({
    mutationFn: async (message?: string) => unwrap(await commands.stashSave(id, message ?? null)),
    onSuccess: (outcome) => {
      invalidate(qc, id, ['status', 'stashes', 'log'])
      if (outcome === 'nothing_to_stash') {
        toast.info('Nothing to stash -- your working tree is already clean.')
      } else {
        toast('Stashed changes')
      }
    },
    onError,
  })

  const stashPop = useMutation({
    mutationFn: async (index: number) => unwrap(await commands.stashPop(id, index)),
    onSuccess: () => {
      invalidate(qc, id, ['status', 'stashes', 'log'])
      toast('Applied stash and removed it from your stash list')
    },
    onError,
  })

  const stashApply = useMutation({
    mutationFn: async (index: number) => unwrap(await commands.stashApply(id, index)),
    onSuccess: () => {
      invalidate(qc, id, ['status', 'stashes', 'log'])
      toast('Applied stash -- a copy stays in your stash list')
    },
    onError,
  })

  const stashDrop = useMutation({
    mutationFn: async (index: number) => unwrap(await commands.stashDrop(id, index)),
    onSuccess: () => {
      invalidate(qc, id, ['status', 'stashes', 'log'])
      toast('Deleted stash')
    },
    onError,
  })

  const fetch = useMutation({
    mutationFn: async () => unwrap(await commands.gitFetch(id)),
    onSuccess: () => {
      invalidate(qc, id, ['log', 'branches', 'remotes'])
      toast('Fetched all remotes')
    },
    onError,
  })

  const addRemote = useMutation({
    mutationFn: async (args: { name: string; url: string }) =>
      unwrap(await commands.addRemote(id, args.name, args.url)),
    onSuccess: (_d, args) => {
      invalidate(qc, id, ['remotes'])
      toast(`Added remote ${args.name}`)
    },
    onError,
  })

  const renameRemote = useMutation({
    mutationFn: async (args: { name: string; newName: string }) =>
      unwrap(await commands.renameRemote(id, args.name, args.newName)),
    onSuccess: (_d, args) => {
      invalidate(qc, id, REMOTE_REFS)
      toast(`Renamed to ${args.newName}`)
    },
    onError,
  })

  const setRemoteUrl = useMutation({
    mutationFn: async (args: { name: string; url: string }) =>
      unwrap(await commands.setRemoteUrl(id, args.name, args.url)),
    onSuccess: (_d, args) => {
      invalidate(qc, id, ['remotes'])
      toast(`Updated ${args.name}`)
    },
    onError,
  })

  const removeRemote = useMutation({
    mutationFn: async (name: string) => {
      await unwrap(await commands.removeRemote(id, name))
      return name
    },
    onSuccess: (name) => {
      invalidate(qc, id, REMOTE_REFS)
      toast(`Removed remote ${name}`)
    },
    onError,
  })

  const setUpstream = useMutation({
    mutationFn: async (remoteBranch: string) => {
      await unwrap(await commands.setUpstream(id, remoteBranch))
      return remoteBranch
    },
    onSuccess: (remoteBranch) => {
      invalidate(qc, id, ['branches'])
      toast(`Now tracking ${remoteBranch}`)
    },
    onError,
  })

  const pull = useMutation({
    mutationFn: async () => unwrap(await commands.gitPull(id)),
    onSuccess: (result) => {
      toast(describePull(result, hostOf(result.upstream)))
    },
    onError,
    // A conflicting pull exits as an error but leaves a merge or rebase in
    // progress, so merge state must refresh on both outcomes for the banner
    // and conflict view to appear.
    onSettled: () => {
      invalidate(qc, id, ['status', 'log', 'branches', 'mergeState'])
    },
  })

  const push = useMutation({
    mutationFn: async () => unwrap(await commands.gitPush(id)),
    onSuccess: (result) => {
      invalidate(qc, id, REFS)
      toast(describePush(result, hostOf(result.upstream)))
      void handleTagsAfterPush()
    },
    onError,
  })

  // Push a branch by name, which may not be the one checked out. A branch with
  // no upstream gets published and tracked in the same step.
  const pushBranch = useMutation({
    mutationFn: async (branch: string) => unwrap(await commands.gitPushBranch(id, branch)),
    onSuccess: (result) => {
      invalidate(qc, id, REFS)
      toast(describePush(result, hostOf(result.upstream)))
      void handleTagsAfterPush()
    },
    onError,
  })

  // Bring a branch up to date without checking it out. Only fast-forwards; a
  // diverged branch is refused with an explanation.
  const pullBranch = useMutation({
    mutationFn: async (branch: string) => unwrap(await commands.gitPullBranch(id, branch)),
    onSuccess: (result) => {
      invalidate(qc, id, ['status', 'log', 'branches'])
      toast(describePull(result, hostOf(result.upstream)))
    },
    onError,
  })

  const renameBranch = useMutation({
    mutationFn: async (v: { name: string; newName: string }) =>
      unwrap(await commands.renameBranch(id, v.name, v.newName)),
    onSuccess: (_r, v) => {
      invalidate(qc, id, REFS)
      toast(`Renamed to ${v.newName}`)
    },
    onError,
  })

  // Distinct from `setUpstream`, which points HEAD at a remote branch the user
  // picked. This links a named branch to the remote branch of the same name.
  const reconnectBranch = useMutation({
    mutationFn: async (branch: string) => unwrap(await commands.setBranchUpstream(id, branch, null)),
    onSuccess: (upstream) => {
      invalidate(qc, id, ['branches'])
      toast(`Now linked to ${upstream}`)
    },
    onError,
  })

  const pushForce = useMutation({
    mutationFn: async () => unwrap(await commands.gitPushForce(id)),
    onSuccess: (result) => {
      invalidate(qc, id, REFS)
      const host = hostOf(result.upstream)
      toast(
        result.pushed === 0
          ? `Force-push finished - ${describeTarget(result, host)} already matched`
          : `Force-pushed ${commitCount(result.pushed)} to ${describeTarget(result, host)}`
      )
    },
    onError,
  })

  const rebase = useMutation({
    mutationFn: async (args: { onto: string; branch?: string }) => ({
      onto: args.onto,
      result: unwrap(await commands.gitRebase(id, args.onto, args.branch ?? null)),
    }),
    onSuccess: ({ onto, result }) => {
      invalidate(qc, id, ['status', 'log', 'branches', 'mergeState'])
      if (result.conflicts.length > 0) {
        toast.warning(
          `Rebase paused on ${plural(result.conflicts.length, 'conflict')} - resolve them, then continue the rebase`
        )
      } else {
        toast(`Rebased onto ${onto}`)
      }
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
          `Merged ${reference} with ${plural(result.conflicts.length, 'conflict')} to resolve`
        )
      } else {
        toast(`Merged ${reference}`)
      }
    },
    onError,
  })

  const mergeDirectional = useMutation({
    mutationFn: async (args: { target: string; source: string }) => ({
      ...args,
      result: unwrap(await commands.mergeDirectional(id, args.target, args.source)),
    }),
    onSuccess: ({ target, source, result }) => {
      invalidate(qc, id, ['status', 'log', 'branches', 'mergeState'])
      if (result.up_to_date) {
        toast(`${target} already contains ${source}`)
      } else if (result.fast_forwarded) {
        toast(`Fast-forwarded ${target} to ${source}`)
      } else if (result.conflicts.length > 0) {
        toast.warning(
          `Merged ${source} into ${target} with ${plural(result.conflicts.length, 'conflict')} to resolve`
        )
      } else {
        toast(`Merged ${source} into ${target}`)
      }
    },
    onError,
  })

  const cherryPick = useMutation({
    mutationFn: async (sha: string) => ({
      sha,
      result: unwrap(await commands.cherryPick(id, sha)),
    }),
    onSuccess: ({ sha, result }) => {
      invalidate(qc, id, ['status', 'log', 'branches', 'mergeState'])
      const short = shortSha(sha)
      if (result.conflicts.length > 0) {
        toast.warning(
          `Cherry-pick of ${short} hit ${plural(result.conflicts.length, 'conflict')} to resolve`
        )
      } else {
        toast(`Cherry-picked ${short}`)
      }
    },
    onError,
  })

  // After a ref move (reset/move-branch), refresh history and offer an undo
  // that puts the branch back where it was.
  const afterRefMove = (previousSha: string, verb: string, undoMode: ResetMode) => {
    invalidate(qc, id, ['status', 'log', 'branches'])
    toast(`${verb} ${shortSha(previousSha)}`, {
      action: {
        label: 'Undo',
        onClick: () => {
          void commands.resetCurrent(id, previousSha, undoMode).then((r) => {
            if (r.status === 'ok') invalidate(qc, id, ['status', 'log', 'branches'])
            else toast.error(r.error)
          })
        },
      },
    })
  }

  const reset = useMutation({
    mutationFn: async (args: { sha: string; mode: ResetMode }) => ({
      mode: args.mode,
      move: unwrap(await commands.resetCurrent(id, args.sha, args.mode)),
    }),
    onSuccess: ({ mode, move }) =>
      afterRefMove(move.previous_sha, `Rewound ${move.branch} — was at`, mode),
    onError,
  })

  const moveBranch = useMutation({
    mutationFn: async (sha: string) => unwrap(await commands.moveCurrentBranch(id, sha)),
    onSuccess: (move) => afterRefMove(move.previous_sha, `Moved ${move.branch} — was at`, 'Soft'),
    onError,
  })

  const openOnGitHub = useMutation({
    mutationFn: async (sha: string) => {
      const url = unwrap(await commands.commitWebUrl(id, sha))
      if (!url) {
        toast('This commit is not on a supported remote yet')
        return
      }
      const { openUrl } = await import('@tauri-apps/plugin-opener')
      await openUrl(url)
    },
    onError,
  })

  const copyCommitLink = useMutation({
    mutationFn: async (sha: string) => {
      const url = unwrap(await commands.commitWebUrl(id, sha))
      if (!url) {
        toast('This commit is not on a supported remote yet')
        return
      }
      // Reports its own success/failure, so a clipboard problem is not
      // classified as a git error by onError.
      await copyToClipboard(url, 'Copied link to commit')
    },
    onError,
  })

  const checkoutCommit = useMutation({
    mutationFn: async (sha: string) => {
      unwrap(await commands.checkoutCommit(id, sha))
      return sha
    },
    onSuccess: (sha) => {
      invalidate(qc, id, ['status', 'log', 'branches'])
      toast(`Checked out ${shortSha(sha)} — you're not on a branch now`)
    },
    onError,
  })

  const rewordCommit = useMutation({
    mutationFn: async (args: { sha: string; message: string }) =>
      unwrap(await commands.rewordCommit(id, args.sha, args.message)),
    onSuccess: (sha) => {
      invalidate(qc, id, ['status', 'log', 'branches'])
      toast(`Updated message — now ${shortSha(sha)}`)
    },
    onError,
  })

  const revertCommit = useMutation({
    mutationFn: async (sha: string) => ({
      sha,
      result: unwrap(await commands.revertCommit(id, sha)),
    }),
    onSuccess: ({ sha, result }) => {
      invalidate(qc, id, ['status', 'log', 'branches', 'mergeState'])
      const short = shortSha(sha)
      if (result.conflicts.length > 0) {
        toast.warning(
          `Reverting ${short} hit ${plural(result.conflicts.length, 'conflict')} to resolve`
        )
      } else {
        toast(`Reverted ${short}`)
      }
    },
    onError,
  })

  const dropCommit = useMutation({
    mutationFn: async (sha: string) => unwrap(await commands.dropCommit(id, sha)),
    onSuccess: (move) => afterRefMove(move.previous_sha, `Dropped commit — was at`, 'Hard'),
    onError,
  })

  const rebaseContinue = useMutation({
    mutationFn: async () => unwrap(await commands.rebaseContinue(id)),
    onSuccess: (result) => {
      invalidate(qc, id, ['status', 'log', 'branches', 'mergeState'])
      if (result.conflicts.length > 0) {
        toast.warning(
          `The next step hit ${plural(result.conflicts.length, 'conflict')} - resolve them to keep going`
        )
      } else {
        toast('Rebase finished')
      }
    },
    onError,
  })

  const rebaseAbort = useMutation({
    mutationFn: async () => unwrap(await commands.rebaseAbort(id)),
    onSuccess: () => {
      invalidate(qc, id, ['status', 'log', 'branches', 'mergeState'])
      toast('Rebase abandoned - your branch is back where it started')
    },
    onError,
  })

  const abortMerge = useMutation({
    mutationFn: async () => unwrap(await commands.abortMerge(id)),
    onSuccess: () => {
      invalidate(qc, id, ['status', 'log', 'branches', 'mergeState'])
      toast('Operation aborted')
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
      toast(`Committed ${shortSha(sha)}`)
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
    stageFiles,
    unstageFile,
    unstageFiles,
    stageAll,
    unstageAll,
    discardFile,
    discardFiles,
    discardAll,
    addToGitignore,
    updateSubmodule,
    createCommit,
    createBranch,
    deleteBranch,
    createTag,
    deleteTag,
    pushTag,
    deleteRemoteTag,
    revealInFileManager,
    openInEditor,
    openInTerminal,
    checkout,
    stashSave,
    stashPop,
    stashApply,
    stashDrop,
    fetch,
    pull,
    push,
    pushBranch,
    pullBranch,
    renameBranch,
    reconnectBranch,
    pushForce,
    rebase,
    addRemote,
    renameRemote,
    setRemoteUrl,
    removeRemote,
    setUpstream,
    merge,
    mergeDirectional,
    cherryPick,
    reset,
    moveBranch,
    openOnGitHub,
    copyCommitLink,
    checkoutCommit,
    rewordCommit,
    revertCommit,
    dropCommit,
    abortMerge,
    rebaseContinue,
    rebaseAbort,
    resolveConflict,
    commitMerge,
    stageLines,
    unstageLines,
    discardLines,
  }
}
