import {
  Copy,
  Download,
  ExternalLink,
  GitBranch,
  GitMerge,
  GitPullRequestArrow,
  RefreshCw,
  Repeat2,
  RotateCcw,
  Trash2,
} from 'lucide-react'
import type { RemoteInfo } from '@/lib/bindings'
import {
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuShortcut,
  ContextMenuSub,
  ContextMenuSubContent,
  ContextMenuSubTrigger,
} from '@/components/ui/context-menu'
import { PendingMenuItem } from '@/components/ui/pending-menu-item'
import { useGitMutations } from '@/hooks/useGitMutations'
import { useBranches } from '@/hooks/useGitQueries'
import { branchSync } from '@/lib/branchActions'
import { useGithubPrForBranch } from '@/hooks/useGithub'
import { useUiStore } from '@/stores/uiStore'
import { copyToClipboard } from '@/lib/clipboard'
import { openWebUrl, remoteBranchWebUrl, remoteWebTarget } from '@/lib/remoteWeb'
import type { PreviewMode } from '@/lib/syncPreview'

interface RemoteBranchMenuItemsProps {
  /** The remote this branch lives on. */
  remote: RemoteInfo
  /** The branch as it exists on the remote, with no remote prefix. */
  branch: string
  repoId: string | null
  /** The local branch of the same name, when there is one. */
  localCounterpart?: string | null
  /** Tip commit of the remote branch, for "copy commit ID". */
  tip?: string | null
  /** Set while a merge or similar is mid-flight, which blocks remote work. */
  opInProgress?: boolean
}

/**
 * Every action that applies to a branch on a remote, as menu items.
 *
 * A remote branch is not a local branch, so it gets its own set rather than
 * borrowing the local one: you cannot rename it, and switching to it means
 * making a local copy first. The actions git genuinely cannot do here are left
 * out entirely rather than shown disabled.
 *
 * Where a local counterpart exists, the caller renders [`BranchMenu`] after
 * these so both halves are reachable from the one right-click -- "get the
 * commits from origin/foo" and "send local foo" are different actions and the
 * user may want either.
 */
export function RemoteBranchMenuItems({
  remote,
  branch,
  repoId,
  localCounterpart,
  tip,
  opInProgress,
}: RemoteBranchMenuItemsProps) {
  const m = useGitMutations(repoId)
  const branches = useBranches(repoId)
  const openRemoteSync = useUiStore((s) => s.openRemoteSync)
  const deleteRemoteBranchPrompt = useUiStore((s) => s.deleteRemoteBranchPrompt)

  const fullName = `${remote.name}/${branch}`
  const head = branches.data?.local.find((b) => b.is_head)
  const currentBranch = head?.name ?? ''
  // Standing on the local copy of this branch. Switching to it again is a
  // no-op, but that says nothing about whether the two point at the same
  // COMMIT -- being on `main` while `origin/main` has moved ahead is the whole
  // reason merge and rebase exist.
  const isCheckedOut = !!localCounterpart && localCounterpart === currentBranch

  // How the checked-out branch stands against this exact remote branch. Only
  // meaningful when this remote branch is HEAD's own upstream; otherwise the
  // two are unrelated refs and the generic items apply.
  const isUpstreamOfHead = !!head?.upstream && head.upstream === fullName
  const headSync = head && isUpstreamOfHead ? branchSync(head) : null
  // Commits the remote has that we don't. Drives merge/rebase: with nothing to
  // get, both are genuinely no-ops and stay disabled.
  const behind = headSync?.behind ?? 0
  const ahead = headSync?.ahead ?? 0
  // Same name AND same commit: nothing to bring across in either direction.
  const isSameCommit = isCheckedOut && isUpstreamOfHead && behind === 0 && ahead === 0
  const isRebasing = m.rebase.isPending && m.rebase.variables?.onto === fullName

  const pr = useGithubPrForBranch(repoId, branch)
  const webTarget = remoteWebTarget(remote)
  const webUrl = webTarget ? remoteBranchWebUrl(webTarget, branch) : null

  const isSwitching = m.checkout.isPending && m.checkout.variables === fullName
  const isCreating = m.createBranch.isPending && m.createBranch.variables?.name === branch
  const resetting = m.resetToBranch.isPending && m.resetToBranch.variables?.target === fullName
  const busy = opInProgress || m.checkout.isPending

  // Same argument order as a drag: the remote ref is the one "picked up". It
  // cannot move, so the Sync window resolves the local branch as the side that
  // receives -- which is what every item here means.
  const openSync = (source: string, mode: PreviewMode) =>
    openRemoteSync(source, currentBranch, mode)

  return (
    <>
      {/* Getting the branch onto this computer. With no local copy the branch
          is download-only, so both ways of grabbing it lead the menu. */}
      {!isCheckedOut && (
        <PendingMenuItem
          icon={<GitBranch />}
          label={localCounterpart ? `Switch to ${localCounterpart}` : `Check out ${branch}`}
          pendingLabel="Switching…"
          pending={isSwitching}
          disabled={busy}
          onRun={() => m.checkout.mutate(fullName)}
        />
      )}
      {!localCounterpart && (
        <PendingMenuItem
          icon={<Download />}
          label={`Make a local copy of ${branch}`}
          pendingLabel="Copying…"
          pending={isCreating}
          disabled={busy || !tip || m.createBranch.isPending}
          onRun={() => m.createBranch.mutate({ name: branch, sha: tip ?? undefined, checkout: false })}
        />
      )}
      <PendingMenuItem
        icon={<RefreshCw />}
        label={`Check ${remote.name} for new work`}
        pendingLabel="Checking…"
        pending={m.fetch.isPending}
        disabled={opInProgress || m.fetch.isPending}
        onRun={() => m.fetch.mutate()}
      />
      <ContextMenuSeparator />

      {/* Straight-line catch-up: nothing of our own is in the way, so this
          needs no merge decision at all. Offered ahead of blend/stack so the
          easy case stays the obvious one. */}
      {behind > 0 && ahead === 0 && (
        <PendingMenuItem
          icon={<Download />}
          label={`Get ${behind} change${behind === 1 ? '' : 's'} from ${remote.name}`}
          pendingLabel="Getting…"
          pending={m.pull.isPending}
          disabled={opInProgress || m.pull.isPending}
          onRun={() => m.pull.mutate()}
        />
      )}

      {/* Bringing its commits into the branch you are on. Standing on the
          local branch of the same name does NOT rule these out -- that is the
          usual case for "origin/main moved ahead of my main". Both stay live
          whenever the remote has commits we don't. */}
      {/* All three open the Sync window on the matching option, so the choice
          between them is made once, in one place, with the outcome drawn. */}
      <ContextMenuItem
        disabled={isSameCommit || opInProgress}
        onSelect={() => openSync(fullName, 'blend')}
      >
        <GitMerge />
        <div className="flex flex-col">
          <span>Blend into {currentBranch || 'current'}</span>
          {behind > 0 && (
            <span className="text-2xs text-muted-foreground">
              Brings {behind} change{behind === 1 ? '' : 's'} down
            </span>
          )}
        </div>
      </ContextMenuItem>
      {behind > 0 && ahead > 0 && (
        <ContextMenuItem
          disabled={opInProgress}
          onSelect={() => openSync(fullName, 'stack')}
        >
          <Repeat2 />
          <div className="flex flex-col">
            <span>Stack my {ahead} change{ahead === 1 ? '' : 's'} on top</span>
            <span className="text-2xs text-muted-foreground">Replays your work, no merge commit</span>
          </div>
        </ContextMenuItem>
      )}
      {!isSameCommit && (
        <ContextMenuItem
          variant="destructive"
          disabled={opInProgress || resetting}
          onSelect={() => openSync(fullName, 'reset')}
        >
          <RotateCcw />
          <div className="flex flex-col">
            <span>{resetting ? 'Resetting…' : `Make ${currentBranch || 'current'} match this`}</span>
            <span className="text-2xs opacity-80">Erases your changes here</span>
          </div>
        </ContextMenuItem>
      )}
      <ContextMenuSeparator />

      {pr && (
        <ContextMenuItem onSelect={() => openWebUrl(pr.html_url, 'GitHub')}>
          <GitPullRequestArrow />
          View pull request on GitHub
          <ContextMenuShortcut className="text-2xs">#{pr.number}</ContextMenuShortcut>
        </ContextMenuItem>
      )}
      {webTarget && webUrl && (
        <ContextMenuItem onSelect={() => openWebUrl(webUrl, webTarget.label)}>
          <ExternalLink />
          View on {webTarget.label}
        </ContextMenuItem>
      )}
      <ContextMenuSub>
        <ContextMenuSubTrigger>
          <Copy />
          Copy
        </ContextMenuSubTrigger>
        <ContextMenuSubContent className="w-52">
          <ContextMenuItem onSelect={() => void copyToClipboard(fullName, `Copied ${fullName}`)}>
            Branch name
          </ContextMenuItem>
          {tip && (
            <ContextMenuItem onSelect={() => void copyToClipboard(tip, `Copied ${tip}`)}>
              Latest commit ID
              <ContextMenuShortcut className="font-mono">{tip.slice(0, 7)}</ContextMenuShortcut>
            </ContextMenuItem>
          )}
        </ContextMenuSubContent>
      </ContextMenuSub>
      <ContextMenuSeparator />

      <ContextMenuItem
        variant="destructive"
        disabled={opInProgress || m.deleteRemoteBranch.isPending}
        onSelect={() => deleteRemoteBranchPrompt({ remote: remote.name, branch })}
      >
        <Trash2 />
        Delete from {remote.name}…
      </ContextMenuItem>
    </>
  )
}
