import {
  ArrowDown,
  ArrowUp,
  Copy,
  ExternalLink,
  GitBranch,
  GitMerge,
  GitPullRequestArrow,
  Link2,
  PenLine,
  RotateCcw,
  Trash2,
  Zap,
} from 'lucide-react'
import { useQuery } from '@tanstack/react-query'
import { toast } from 'sonner'
import type { BranchInfo, ResetMode } from '@/lib/bindings'
import { commands } from '@/lib/bindings'
import { unwrap } from '@/lib/queryKeys'
import {
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuShortcut,
  ContextMenuSub,
  ContextMenuSubContent,
  ContextMenuSubTrigger,
} from '@/components/ui/context-menu'
import { PendingIndicator } from '@/components/ui/pending-indicator'
import { PendingMenuItem } from '@/components/ui/pending-menu-item'
import { useGitMutations } from '@/hooks/useGitMutations'
import { useBranchHost, useRemotes } from '@/hooks/useGitQueries'
import { branchActions } from '@/lib/branchActions'
import { copyToClipboard } from '@/lib/clipboard'
import { openWebUrl, remoteBranchWebUrl, remoteWebTarget } from '@/lib/remoteWeb'

export interface BranchMenuHandlers {
  onMerge: (name: string) => void
  onRename: (name: string) => void
  onDelete: (name: string) => void
  /** Confirm hard-resetting the current branch to this branch. */
  onResetTo: (name: string) => void
}

interface BranchMenuItemsProps {
  branch: BranchInfo
  repoId: string | null
  /** The checked-out branch, used to word and gate the merge item. */
  currentBranch: string
  /** Set while a merge or similar is mid-flight, which blocks remote work. */
  opInProgress?: boolean
  showWebLink?: boolean
  handlers: BranchMenuHandlers
}

/**
 * Every action that applies to a local branch, as menu items.
 *
 * Right-clicking a branch offers the same things wherever you do it -- the
 * sidebar row, a chip in the graph, a commit at the branch tip, or the remote
 * branch it tracks. Only the actions git cannot perform are left out: you
 * cannot merge or delete the branch you are on, and push/pull appear only when
 * there is something to send or get.
 */
export function BranchMenuItems({
  branch,
  repoId,
  currentBranch,
  opInProgress,
  showWebLink = true,
  handlers,
}: BranchMenuItemsProps) {
  const m = useGitMutations(repoId)
  const host = useBranchHost(repoId, branch.upstream)
  const remotes = useRemotes(repoId)
  const actions = branchActions(branch, host)
  const isCurrent = branch.is_head

  // How this branch relates to the checked-out one: ahead = commits only this
  // branch has, behind = commits only the current branch has. Drives the two
  // fast-forward affordances below. Skipped for the current branch itself (it
  // can't merge or fast-forward into itself).
  const relation = useQuery({
    queryKey: ['branchRelation', repoId, branch.name, currentBranch],
    enabled: !!repoId && !isCurrent && !!currentBranch,
    queryFn: async () => unwrap(await commands.branchRelation(repoId!, branch.name, currentBranch)),
  })
  // This branch trails the current one on a straight line: it can catch up with
  // a plain fast-forward (no switch, no merge commit) -- the case that had no
  // menu item before.
  const canFastForwardToCurrent =
    !!relation.data && relation.data.ahead === 0 && relation.data.behind > 0
  // The reverse: current trails this branch on a straight line, so "Merge into
  // current" is really a fast-forward. Reword it so that reads clearly.
  const mergeIsFastForward =
    !!relation.data && relation.data.behind === 0 && relation.data.ahead > 0
  const isFastForwarding =
    m.fastForwardBranch.isPending && m.fastForwardBranch.variables?.branch === branch.name

  const isPushing = m.pushBranch.isPending && m.pushBranch.variables === branch.name
  const isPulling = m.pullBranch.isPending && m.pullBranch.variables === branch.name
  const isSwitching = m.checkout.isPending && m.checkout.variables === branch.name
  const busy = m.pushBranch.isPending || m.pullBranch.isPending

  // Reset rewinds the checked-out branch TO this one, so it only makes sense on
  // some other branch. Soft/Mixed keep your files, so they run straight away;
  // Hard erases uncommitted work, so it goes through a confirm.
  const resetting = m.resetToBranch.isPending && m.resetToBranch.variables?.target === branch.name
  const resetTo = (mode: ResetMode) => m.resetToBranch.mutate({ target: branch.name, mode })
  const isResetMode = (mode: ResetMode) => resetting && m.resetToBranch.variables?.mode === mode

  const hasRemoteAction = actions.push.show || actions.pull.show || actions.setUpstream.show
  const [upstreamRemoteName, ...upstreamBranchParts] = branch.upstream?.split('/') ?? []
  const upstreamRemote = remotes.data?.find((remote) => remote.name === upstreamRemoteName)
  const webTarget = upstreamRemote ? remoteWebTarget(upstreamRemote.url) : null
  const webUrl = webTarget && upstreamBranchParts.length > 0
    ? remoteBranchWebUrl(webTarget, upstreamBranchParts.join('/'))
    : null

  return (
    <>
      {actions.push.show && (
        <PendingMenuItem
          icon={<ArrowUp />}
          label={actions.push.label}
          pendingLabel="Sending…"
          pending={isPushing}
          disabled={opInProgress || busy}
          onRun={() => m.pushBranch.mutate(branch.name)}
        />
      )}
      {actions.pull.show && (
        <PendingMenuItem
          icon={<ArrowDown />}
          label={actions.pull.label}
          pendingLabel="Getting…"
          pending={isPulling}
          disabled={opInProgress || busy}
          onRun={() => m.pullBranch.mutate(branch.name)}
        />
      )}
      {actions.setUpstream.show && (
        <PendingMenuItem
          icon={<Link2 />}
          label={actions.setUpstream.label}
          pendingLabel="Reconnecting…"
          pending={m.reconnectBranch.isPending}
          disabled={m.reconnectBranch.isPending}
          onRun={() => m.reconnectBranch.mutate(branch.name)}
        />
      )}
      {hasRemoteAction && <ContextMenuSeparator />}

      {!isCurrent && (
        <PendingMenuItem
          icon={<GitBranch />}
          label={`Switch to ${branch.name}`}
          pendingLabel={`Switching to ${branch.name}…`}
          pending={isSwitching}
          disabled={opInProgress || m.checkout.isPending}
          onRun={() => m.checkout.mutate(branch.name)}
        />
      )}
      <ContextMenuItem disabled={isCurrent} onSelect={() => handlers.onMerge(branch.name)}>
        {mergeIsFastForward ? <Zap /> : <GitMerge />}
        {mergeIsFastForward
          ? `Fast-forward ${currentBranch || 'current'} to ${branch.name}`
          : `Merge into ${currentBranch || 'current'}`}
      </ContextMenuItem>
      {canFastForwardToCurrent && (
        <PendingMenuItem
          icon={<Zap />}
          label={`Fast-forward ${branch.name} to ${currentBranch || 'current'}`}
          pendingLabel={`Catching ${branch.name} up…`}
          pending={isFastForwarding}
          disabled={opInProgress || m.fastForwardBranch.isPending}
          onRun={() => m.fastForwardBranch.mutate({ branch: branch.name, target: currentBranch })}
        />
      )}
      {!isCurrent && (
        <ContextMenuSub>
          <ContextMenuSubTrigger
            className="data-[disabled]:pointer-events-none data-[disabled]:opacity-50"
            data-disabled={opInProgress || resetting ? '' : undefined}
          >
            <RotateCcw />
            Reset {currentBranch || 'current'} to {branch.name}
          </ContextMenuSubTrigger>
          <ContextMenuSubContent className="w-64">
            <ContextMenuItem
              disabled={resetting}
              onSelect={(e) => {
                e.preventDefault()
                resetTo('Mixed')
              }}
            >
              {isResetMode('Mixed') && <PendingIndicator />}
              <div className="flex flex-col">
                <span>{isResetMode('Mixed') ? 'Resetting…' : 'Match it, keep your changes'}</span>
                <span className="text-2xs text-muted-foreground">Changes stay in your files</span>
              </div>
            </ContextMenuItem>
            <ContextMenuItem
              disabled={resetting}
              onSelect={(e) => {
                e.preventDefault()
                resetTo('Soft')
              }}
            >
              {isResetMode('Soft') && <PendingIndicator />}
              <div className="flex flex-col">
                <span>{isResetMode('Soft') ? 'Resetting…' : 'Match it, keep changes staged'}</span>
                <span className="text-2xs text-muted-foreground">Changes stay ready to commit</span>
              </div>
            </ContextMenuItem>
            <ContextMenuItem
              variant="destructive"
              disabled={resetting}
              onSelect={() => handlers.onResetTo(branch.name)}
            >
              <div className="flex flex-col">
                <span>Match it, erase your changes</span>
                <span className="text-2xs opacity-80">Can't be undone easily</span>
              </div>
            </ContextMenuItem>
          </ContextMenuSubContent>
        </ContextMenuSub>
      )}
      {showWebLink && webTarget && webUrl && (
        <ContextMenuItem onSelect={() => openWebUrl(webUrl, webTarget.label)}>
          <ExternalLink />
          View on {webTarget.label}
        </ContextMenuItem>
      )}
      {/* TODO(github): needs the GitHub integration before it can run. */}
      <ContextMenuItem onSelect={() => toast('GitHub integration is planned')}>
        <GitPullRequestArrow />
        Start a pull request
        <ContextMenuShortcut className="text-2xs">soon</ContextMenuShortcut>
      </ContextMenuItem>
      <ContextMenuSeparator />

      <ContextMenuItem onSelect={() => handlers.onRename(branch.name)}>
        <PenLine />
        Rename branch…
      </ContextMenuItem>
      <ContextMenuSub>
        <ContextMenuSubTrigger>
          <Copy />
          Copy
        </ContextMenuSubTrigger>
        <ContextMenuSubContent className="w-52">
          <ContextMenuItem
            onSelect={() => void copyToClipboard(branch.name, `Copied ${branch.name}`)}
          >
            Branch name
          </ContextMenuItem>
          {branch.tip && (
            <ContextMenuItem
              onSelect={() => void copyToClipboard(branch.tip ?? '', `Copied ${branch.tip}`)}
            >
              Latest commit ID
              <ContextMenuShortcut className="font-mono">{branch.tip}</ContextMenuShortcut>
            </ContextMenuItem>
          )}
          {branch.upstream && (
            <ContextMenuItem
              onSelect={() =>
                void copyToClipboard(branch.upstream ?? '', 'Copied remote branch name')
              }
            >
              Remote branch name
            </ContextMenuItem>
          )}
        </ContextMenuSubContent>
      </ContextMenuSub>
      <ContextMenuSeparator />

      <ContextMenuItem
        variant="destructive"
        disabled={isCurrent}
        onSelect={() => handlers.onDelete(branch.name)}
      >
        <Trash2 />
        Delete branch
      </ContextMenuItem>
    </>
  )
}
