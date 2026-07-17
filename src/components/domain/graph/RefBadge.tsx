import { type DragEvent } from 'react'
import { cn } from '@/lib/utils'
import type { RefInfo, RefKind } from '@/lib/bindings'
import { REF_DND_MIME, resolveSyncPair, type DraggedRef } from '@/lib/refSync'
import { detectProvider, RemoteIcon } from '@/lib/remoteProvider'
import { useBranches, useRemotes } from '@/hooks/useGitQueries'
import { useUiStore } from '@/stores/uiStore'
import { useDragStore } from '@/stores/dragStore'
import { useActiveRepo } from '@/stores/workspaceStore'
import { RefContextMenu } from './RefContextMenu'

const styles: Record<RefKind, string> = {
  head: 'bg-primary text-primary-foreground',
  branch: 'border border-primary text-primary',
  remote: 'border border-border text-sub',
  tag: 'bg-modified text-[#1a1400]',
}

export function RefBadge({ refTag }: { refTag: RefInfo }) {
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

  // Would this pill accept whatever is currently being dragged?
  const isValidTarget =
    !!draggingRef &&
    draggingRef.name !== refTag.name &&
    !!branches.data &&
    !!resolveSyncPair(draggingRef, self, branches.data)

  const onDragStart = (e: DragEvent) => {
    e.dataTransfer.setData(REF_DND_MIME, JSON.stringify(self))
    e.dataTransfer.effectAllowed = 'move'
    startDrag(self)
  }

  const onDragEnd = () => endDrag()

  const onDragOver = (e: DragEvent) => {
    // Accept the drop when the live drag forms a valid tracking pair. Calling
    // preventDefault is what turns the cursor from "no-drop" into "move".
    if (!isValidTarget) return
    e.preventDefault()
    e.dataTransfer.dropEffect = 'move'
  }

  const onDrop = (e: DragEvent) => {
    if (!draggingRef || !isValidTarget) return
    e.preventDefault()
    openRemoteSync(draggingRef.name, refTag.name)
    endDrag()
  }

  // While something is being dragged, dim pills that can't accept it so the
  // valid targets stand out.
  const dragging = !!draggingRef
  const isSource = draggingRef?.name === refTag.name

  return (
    <RefContextMenu refTag={refTag}>
      <span
        draggable={draggable}
        onDragStart={onDragStart}
        onDragEnd={onDragEnd}
        onDragOver={onDragOver}
        onDrop={onDrop}
        className={cn(
          'inline-flex max-w-[110px] flex-none items-center gap-1 overflow-hidden rounded-[5px] px-1.5 py-px font-mono text-[9.5px] font-semibold leading-[1.4] transition-[box-shadow,opacity]',
          draggable ? 'cursor-grab active:cursor-grabbing' : 'cursor-default',
          styles[refTag.type],
          isValidTarget && 'ring-2 ring-added ring-offset-1 ring-offset-background',
          dragging && !isValidTarget && !isSource && 'opacity-40',
          isSource && 'opacity-70'
        )}
      >
        {refTag.type === 'remote' && (
          <RemoteIcon provider={provider} width={9} height={9} className="flex-none" />
        )}
        <span className="overflow-hidden text-ellipsis whitespace-nowrap">{label}</span>
      </span>
    </RefContextMenu>
  )
}
