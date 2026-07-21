import { type DragEvent } from 'react'
import { Check, Laptop, Tag } from 'lucide-react'
import { cn } from '@/lib/utils'
import type { RefInfo, RefKind } from '@/lib/bindings'
import { REF_DND_MIME, resolveDropPair, type DraggedRef } from '@/lib/refSync'
import { detectProvider, RemoteIcon } from '@/lib/remoteProvider'
import { useBranches, useRemotes } from '@/hooks/useGitQueries'
import { useUiStore } from '@/stores/uiStore'
import { useDragStore } from '@/stores/dragStore'
import { useActiveRepo } from '@/stores/workspaceStore'
import { RefContextMenu } from './RefContextMenu'

// Every pill shares the accent background so the drag ghost always looks like
// the thing being dragged, whichever kind of ref it is.
const styles: Record<RefKind, string> = {
  head: 'bg-primary text-primary-foreground',
  branch: 'bg-primary text-primary-foreground',
  remote: 'bg-primary text-primary-foreground',
  tag: 'bg-primary text-primary-foreground',
}

export function RefBadge({
  refTag,
  withContextMenu = true,
}: {
  refTag: RefInfo
  withContextMenu?: boolean
}) {
  const repo = useActiveRepo()
  const branches = useBranches(repo?.id ?? null)
  const remotes = useRemotes(repo?.id ?? null)
  const openRemoteSync = useUiStore((s) => s.openRemoteSync)
  const draggingRef = useDragStore((s) => s.draggingRef)
  const startDrag = useDragStore((s) => s.startDrag)
  const endDrag = useDragStore((s) => s.endDrag)

  const self: DraggedRef = { name: refTag.name, type: refTag.type }

  // Tags can't sync; everything else can be dragged.
  const draggable = refTag.type !== 'tag'

  // Resolve the remote this pill belongs to (for a remote pill) or that a local
  // pill tracks, so we can show the provider logo and shorten the label.
  const remoteName = refTag.type === 'remote' ? refTag.name.split('/')[0] : null
  const remoteList = remotes.data ?? []
  const remoteInfo = remoteName ? remoteList.find((r) => r.name === remoteName) : null
  const provider = detectProvider(remoteInfo?.url)
  const hasProviderIcon = refTag.type === 'remote' && provider !== 'unknown'

  // Label: for a remote pill, drop the `origin/` prefix when the provider logo
  // already signals which remote it is, or when there's only a single remote so
  // the prefix carries no information. With multiple remotes and no logo to tell
  // them apart, keep the full name.
  const label =
    remoteName && (hasProviderIcon || remoteList.length <= 1)
      ? refTag.name.slice(remoteName.length + 1)
      : refTag.name

  // Would this pill accept the ref currently being dragged? Used both to drive
  // the highlight and to decide whether to accept the drop.
  const acceptsDragged = (dragged: DraggedRef | null) =>
    !!dragged &&
    dragged.name !== refTag.name &&
    !!branches.data &&
    !!resolveDropPair(dragged, self, branches.data)

  const isValidTarget = acceptsDragged(draggingRef)

  const onDragStart = (e: DragEvent) => {
    console.debug('[refdrag] dragstart', {
      self,
      branchesLoaded: !!branches.data,
      locals: branches.data?.local.map((b) => ({
        name: b.name,
        upstream: b.upstream,
        ahead: b.ahead,
        behind: b.behind,
      })),
    })
    e.dataTransfer.setData(REF_DND_MIME, JSON.stringify(self))
    e.dataTransfer.effectAllowed = 'move'

    // Build a visible "ghost" under the cursor. Webviews often render no drag
    // image for a small inline element, so we clone the pill, lift it off-screen,
    // and hand it to setDragImage, then remove it after the browser snapshots it.
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

    startDrag(self)
  }

  const onDragEnd = () => {
    console.debug('[refdrag] dragend', self.name)
    endDrag()
  }

  const onDragOver = (e: DragEvent) => {
    // Only our ref drags carry this MIME type. The payload itself is unreadable
    // during dragover, so accept based on the store's draggingRef; if the store
    // hasn't propagated yet (first event of a drag), accept optimistically so the
    // cursor shows "move" - the drop handler re-validates against the real payload.
    if (!e.dataTransfer.types.includes(REF_DND_MIME)) {
      console.debug('[refdrag] dragover on', self.name, 'rejected: no ref MIME', [
        ...e.dataTransfer.types,
      ])
      return
    }
    if (draggingRef && !isValidTarget) {
      console.debug('[refdrag] dragover on', self.name, 'rejected: not a valid pair for', draggingRef)
      return
    }
    e.preventDefault()
    e.dataTransfer.dropEffect = 'move'
  }

  const onDrop = (e: DragEvent) => {
    // The payload is readable here; validate against it directly so the drop is
    // correct even if the store lagged during dragover.
    const raw = e.dataTransfer.getData(REF_DND_MIME)
    const dragged = raw ? (JSON.parse(raw) as DraggedRef) : draggingRef
    console.debug('[refdrag] drop on', self.name, { raw, dragged, accepts: acceptsDragged(dragged) })
    if (!acceptsDragged(dragged) || !dragged) {
      endDrag()
      return
    }
    e.preventDefault()
    console.debug('[refdrag] opening sync modal', { source: dragged.name, target: refTag.name })
    openRemoteSync(dragged.name, refTag.name)
    endDrag()
  }

  // While something is being dragged, dim pills that can't accept it so the
  // valid targets stand out.
  const dragging = !!draggingRef
  const isSource = draggingRef?.name === refTag.name

  const badge = (
    <span
      draggable={draggable}
      // Radix's context-menu triggers wrap this pill and their pointerdown
      // handling cancels a native drag before it starts. Stop the left-button
      // press from reaching them (right-click still bubbles, so both context
      // menus keep working).
      onPointerDown={(e) => {
        if (draggable && e.button === 0) e.stopPropagation()
      }}
      onDragStart={onDragStart}
      onDragEnd={onDragEnd}
      onDragOver={onDragOver}
      onDrop={onDrop}
      className={cn(
        'inline-flex max-w-[110px] flex-none items-center gap-1 overflow-hidden rounded-[5px] px-1.5 py-px font-mono text-2xs font-semibold leading-[1.4] transition-opacity',
        draggable ? 'wyrm-draggable cursor-grab active:cursor-grabbing' : 'cursor-default',
        styles[refTag.type],
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
    </span>
  )

  return withContextMenu ? <RefContextMenu refTag={refTag}>{badge}</RefContextMenu> : badge
}
