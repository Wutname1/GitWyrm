import { type DragEvent } from 'react'
import { Check, Laptop, Tag } from 'lucide-react'
import { cn } from '@/lib/utils'
import { TooltipHint } from '@/components/ui/tooltip'
import type { RefInfo, RefKind } from '@/lib/bindings'
import { type DraggedRef } from '@/lib/refSync'
import { detectProvider, RemoteIcon } from '@/lib/remoteProvider'
import { tutorialRefPillId } from '@/lib/tutorialLessons'
import { useBranches, useRemotes } from '@/hooks/useGitQueries'
import { useGitMutations } from '@/hooks/useGitMutations'
import { useRefDnd } from '@/hooks/useRefDnd'
import { useActiveRepo } from '@/stores/workspaceStore'
import { RefContextMenu } from './RefContextMenu'

// Every pill shares the accent background so the drag ghost always looks like
// the thing being dragged, whichever kind of ref it is.
const styles: Record<RefKind, string> = {
  head: 'bg-primary text-primary-foreground',
  branch: 'bg-primary text-primary-foreground',
  remote: 'bg-primary text-primary-foreground',
  tag: 'bg-lane1 text-[var(--gw-accent-fg)]',
}

export function RefBadge({
  refTag,
  withContextMenu = true,
  syncedWith,
  expandOnHover = false,
}: {
  refTag: RefInfo
  withContextMenu?: boolean
  /**
   * The remote-tracking ref sitting on this exact commit under the same name.
   * Collapses "main + origin/main, both here" into one chip with a provider
   * glyph instead of a `+1` stack that opens a popover to say the same thing.
   */
  syncedWith?: RefInfo | null
  /** Let the chip grow past its column on hover to reveal a truncated name. */
  expandOnHover?: boolean
}) {
  const repo = useActiveRepo()
  const remotes = useRemotes(repo?.id ?? null)
  const branches = useBranches(repo?.id ?? null)
  const m = useGitMutations(repo?.id ?? null)

  const self: DraggedRef = { name: refTag.name, type: refTag.type }

  // Double-click a local or remote branch chip to go to it. Tags aren't
  // branches, so they don't. `head` is already the current branch.
  const canSwitch = refTag.type === 'branch' || refTag.type === 'remote'

  // A remote chip stands in for the local branch that tracks it, which is what
  // a checkout would land on. When that local branch is the one already
  // checked out, "switch to it" is a no-op -- but double-clicking a remote
  // chip sitting ahead of you plainly means "bring me up to there", so
  // fast-forward instead. Without this the click either did nothing visible or
  // was refused outright as held-by-a-worktree.
  const currentBranch = branches.data?.local.find((b) => b.is_head)
  const localForRemote =
    refTag.type === 'remote'
      ? (branches.data?.local.find((b) => b.upstream === refTag.name)?.name ??
        refTag.name.split('/').slice(1).join('/'))
      : refTag.name
  const isCurrentBranch = !!currentBranch && localForRemote === currentBranch.name
  const willFastForward = canSwitch && isCurrentBranch

  const handleDoubleClick = () => {
    if (!canSwitch || m.checkout.isPending || m.fastForwardBranch.isPending) return
    if (willFastForward) {
      m.fastForwardBranch.mutate({ branch: currentBranch.name, target: refTag.name })
      return
    }
    // The checkout mutation maps a remote-tracking ref (origin/foo) onto its
    // local branch, so passing the full ref name works for either kind.
    m.checkout.mutate(refTag.name)
  }

  // Build a visible "ghost" under the cursor. Webviews often render no drag
  // image for a small inline element, so we clone the pill, lift it off-screen,
  // and hand it to setDragImage, then remove it after the browser snapshots it.
  const buildGhost = (e: DragEvent) => {
    const node = e.currentTarget as HTMLElement
    const ghost = node.cloneNode(true) as HTMLElement
    ghost.style.position = 'fixed'
    ghost.style.top = '-1000px'
    ghost.style.left = '-1000px'
    ghost.style.margin = '0'
    ghost.style.opacity = '0.95'
    ghost.style.transform = 'scale(1.35)'
    ghost.style.pointerEvents = 'none'
    ghost.style.boxShadow = '0 6px 16px rgba(0,0,0,0.55)'
    document.body.appendChild(ghost)
    e.dataTransfer.setDragImage(ghost, 12, 10)
    window.setTimeout(() => ghost.remove(), 0)
  }

  const dnd = useRefDnd(self, buildGhost)
  const { dragging, isSource, isValidTarget } = dnd

  // Resolve the remote this pill belongs to (for a remote pill) or that a local
  // pill tracks, so we can show the provider logo and shorten the label.
  const remoteName = refTag.type === 'remote' ? refTag.name.split('/')[0] : null
  const remoteList = remotes.data ?? []
  const remoteInfo = remoteName ? remoteList.find((r) => r.name === remoteName) : null
  const provider = detectProvider(remoteInfo?.url)
  const hasProviderIcon = refTag.type === 'remote' && provider !== 'unknown'

  // The synced remote's own provider, for the trailing glyph on a merged chip.
  const syncedRemote = syncedWith ? syncedWith.name.split('/')[0] : null
  const syncedProvider = detectProvider(
    remoteList.find((r) => r.name === syncedRemote)?.url
  )

  // Label: for a remote pill, drop the `origin/` prefix when the provider logo
  // already signals which remote it is, or when there's only a single remote so
  // the prefix carries no information. With multiple remotes and no logo to tell
  // them apart, keep the full name.
  const label =
    remoteName && (hasProviderIcon || remoteList.length <= 1)
      ? refTag.name.slice(remoteName.length + 1)
      : refTag.name

  const badge = (
    <span
      {...dnd.props}
      data-tutorial-id={tutorialRefPillId(refTag.name, refTag.type)}
      onDoubleClick={canSwitch ? handleDoubleClick : undefined}
      className={cn(
        'inline-flex max-w-[110px] flex-none items-center gap-1 overflow-hidden rounded-[5px] px-1.5 py-px font-mono text-2xs font-semibold leading-[1.4] transition-opacity',
        dnd.props.draggable ? 'wyrm-draggable cursor-grab active:cursor-grabbing' : 'cursor-default',
        styles[refTag.type],
        // Hovering lifts the chip out of its column so a long name can be read
        // in full over the graph, instead of forcing a widen-the-column detour.
        expandOnHover && 'wyrm-ref-expand',
        // The whole border pulses on a valid target while everything else
        // dims (body.wyrm-dragging), so the landing spots stand out.
        isValidTarget && 'wyrm-drop-target',
        dragging && !isValidTarget && !isSource && 'opacity-30'
      )}
    >
      {refTag.type === 'head' && <Check aria-hidden className="size-2.5 flex-none stroke-[2.5]" />}
      {refTag.type === 'branch' && <Laptop aria-hidden className="size-2.5 flex-none" />}
      {refTag.type === 'remote' && (
        <RemoteIcon aria-hidden provider={provider} width={10} height={10} className="flex-none" />
      )}
      {refTag.type === 'tag' && <Tag aria-hidden className="size-2.5 flex-none" />}
      <span className="overflow-hidden text-ellipsis whitespace-nowrap">{label}</span>
      {syncedWith && (
        <span className="flex flex-none items-center gap-0.5 border-l border-primary-foreground/25 pl-1">
          <RemoteIcon aria-hidden provider={syncedProvider} width={10} height={10} />
        </span>
      )}
    </span>
  )

  const withMenu = withContextMenu ? (
    <RefContextMenu refTag={refTag}>{badge}</RefContextMenu>
  ) : (
    badge
  )

  // Outside the context menu, whose trigger is `asChild`: a tooltip nested
  // inside would become that trigger's target and swallow the right-click menu.
  const hint = syncedWith
    ? `${refTag.name} and ${syncedWith.name} are both here - nothing to push or pull.`
    : willFastForward
      ? `Double-click to catch ${currentBranch.name} up to ${refTag.name}`
      : `Double-click to switch to ${refTag.name}`
  return canSwitch ? <TooltipHint label={hint}>{withMenu}</TooltipHint> : withMenu
}
