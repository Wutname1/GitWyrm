import {
  Copy,
  Download,
  ExternalLink,
  GitBranch,
  GitMerge,
  GitPullRequestArrow,
  RefreshCw,
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
import { useGithubPrForBranch } from '@/hooks/useGithub'
import { useUiStore } from '@/stores/uiStore'
import { copyToClipboard } from '@/lib/clipboard'
import { openWebUrl, remoteBranchWebUrl, remoteWebTarget } from '@/lib/remoteWeb'

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
  const openMerge = useUiStore((s) => s.openMerge)
  const resetToBranchPrompt = useUiStore((s) => s.resetToBranchPrompt)
  const deleteRemoteBranchPrompt = useUiStore((s) => s.deleteRemoteBranchPrompt)

  const fullName = `${remote.name}/${branch}`
  const currentBranch = branches.data?.local.find((b) => b.is_head)?.name ?? ''
  // Already standing on the local copy of this branch: switching to it again is
  // a no-op, and it cannot be merged or reset into itself.
  const isCheckedOut = !!localCounterpart && localCounterpart === currentBranch

  const pr = useGithubPrForBranch(repoId, branch)
  const webTarget = remoteWebTarget(remote)
  const webUrl = webTarget ? remoteBranchWebUrl(webTarget, branch) : null

  const isSwitching = m.checkout.isPending && m.checkout.variables === fullName
  const isCreating = m.createBranch.isPending && m.createBranch.variables?.name === branch
  const resetting = m.resetToBranch.isPending && m.resetToBranch.variables?.target === fullName
  const busy = opInProgress || m.checkout.isPending

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

      {/* Bringing its commits into the branch you are on. */}
      <ContextMenuItem
        disabled={isCheckedOut || opInProgress}
        onSelect={() => openMerge(fullName)}
      >
        <GitMerge />
        Merge into {currentBranch || 'current'}
      </ContextMenuItem>
      {!isCheckedOut && (
        <ContextMenuItem
          variant="destructive"
          disabled={opInProgress || resetting}
          onSelect={() => resetToBranchPrompt(fullName)}
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
