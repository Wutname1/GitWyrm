import type { ReactNode } from 'react'
import type { RefInfo } from '@/lib/bindings'
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuLabel,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from '@/components/ui/context-menu'
import { ExternalLink } from 'lucide-react'
import { useMergeState, useRemotes } from '@/hooks/useGitQueries'
import { useActiveRepo } from '@/stores/workspaceStore'
import { BranchMenu } from '@/components/domain/branch/BranchMenu'
import { openWebUrl, remoteBranchWebUrl, remoteWebTarget } from '@/lib/remoteWeb'

interface RefContextMenuProps {
  refTag: RefInfo
  children: ReactNode
  onOpenChange?: (open: boolean) => void
}

/**
 * Right-click menu for a branch chip in the graph. Offers the same actions as
 * every other branch surface via BranchMenu. Remote chips link to their exact
 * branch on the host; tags fall through to a plain chip.
 *
 * The checked-out branch's chip is tagged `head`, not `branch` -- both are
 * local branches and both get this menu.
 */
export function RefContextMenu({ refTag, children, onOpenChange }: RefContextMenuProps) {
  const repo = useActiveRepo()
  const mergeState = useMergeState(repo?.id ?? null)
  const remotes = useRemotes(repo?.id ?? null)

  const isBranch = refTag.type === 'branch' || refTag.type === 'head'
  const isRemote = refTag.type === 'remote'
  if (!isBranch && !isRemote) return <>{children}</>

  const [remoteName, ...remoteBranchParts] = isRemote ? refTag.name.split('/') : []
  const remote = remotes.data?.find((item) => item.name === remoteName)
  const webTarget = remote ? remoteWebTarget(remote.url) : null
  const webUrl = webTarget && remoteBranchParts.length > 0
    ? remoteBranchWebUrl(webTarget, remoteBranchParts.join('/'))
    : null
  if (isRemote && (!webTarget || !webUrl)) return <>{children}</>

  const opInProgress = mergeState.data?.merging ?? false

  return (
    <ContextMenu onOpenChange={onOpenChange}>
      <ContextMenuTrigger asChild>{children}</ContextMenuTrigger>
      <ContextMenuContent className="z-[70] w-60">
        <ContextMenuLabel className="font-mono text-2xs text-sub">{refTag.name}</ContextMenuLabel>
        <ContextMenuSeparator />
        {isBranch ? (
          <BranchMenu branch={refTag.name} opInProgress={opInProgress} />
        ) : webTarget && webUrl ? (
          <ContextMenuItem onSelect={() => openWebUrl(webUrl, webTarget.label)}>
            <ExternalLink />
            View on {webTarget.label}
          </ContextMenuItem>
        ) : null}
      </ContextMenuContent>
    </ContextMenu>
  )
}
