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
  Trash2,
} from 'lucide-react'
import { toast } from 'sonner'
import type { BranchInfo } from '@/lib/bindings'
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
import { useBranchHost, useRemotes } from '@/hooks/useGitQueries'
import { branchActions } from '@/lib/branchActions'
import { copyToClipboard } from '@/lib/clipboard'
import { openWebUrl, remoteBranchWebUrl, remoteWebTarget } from '@/lib/remoteWeb'

export interface BranchMenuHandlers {
  onMerge: (name: string) => void
  onRename: (name: string) => void
  onDelete: (name: string) => void
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

  const isPushing = m.pushBranch.isPending && m.pushBranch.variables === branch.name
  const isPulling = m.pullBranch.isPending && m.pullBranch.variables === branch.name
  const isSwitching = m.checkout.isPending && m.checkout.variables === branch.name
  const busy = m.pushBranch.isPending || m.pullBranch.isPending

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
        <GitMerge />
        Merge into {currentBranch || 'current'}
      </ContextMenuItem>
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
