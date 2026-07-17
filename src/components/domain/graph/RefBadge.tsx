import { useState, type DragEvent } from 'react'
import { cn } from '@/lib/utils'
import type { RefInfo, RefKind } from '@/lib/bindings'
import { REF_DND_MIME, resolveSyncPair, type DraggedRef } from '@/lib/refSync'
import { useBranches } from '@/hooks/useGitQueries'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo } from '@/stores/workspaceStore'
import { RefContextMenu } from './RefContextMenu'

const styles: Record<RefKind, string> = {
  head: 'bg-primary text-primary-foreground',
  branch: 'border border-primary text-primary',
  remote: 'border border-border text-sub',
  tag: 'bg-modified text-[#1a1400]',
}

/** Read a dragged ref off a drop event, or null if the payload isn't ours. */
function readDraggedRef(e: DragEvent): DraggedRef | null {
  const raw = e.dataTransfer.getData(REF_DND_MIME)
  if (!raw) return null
  try {
    const parsed = JSON.parse(raw) as DraggedRef
    return parsed && typeof parsed.name === 'string' ? parsed : null
  } catch {
    return null
  }
}

export function RefBadge({ refTag }: { refTag: RefInfo }) {
  const repo = useActiveRepo()
  const branches = useBranches(repo?.id ?? null)
  const openRemoteSync = useUiStore((s) => s.openRemoteSync)
  const [dropActive, setDropActive] = useState(false)

  // Local branches and their upstream can take part in a sync drag; tags cannot.
  const draggable = refTag.type !== 'tag'

  const onDragStart = (e: DragEvent) => {
    const payload: DraggedRef = { name: refTag.name, type: refTag.type }
    e.dataTransfer.setData(REF_DND_MIME, JSON.stringify(payload))
    e.dataTransfer.effectAllowed = 'move'
  }

  // Highlight only when the hovered pill would form a valid tracking pair with
  // whatever is being dragged, so the drop target reads as meaningful.
  const pairFor = (dragged: DraggedRef | null) => {
    if (!dragged || dragged.name === refTag.name || !branches.data) return null
    return resolveSyncPair(dragged, { name: refTag.name, type: refTag.type }, branches.data)
  }

  const onDragOver = (e: DragEvent) => {
    if (!e.dataTransfer.types.includes(REF_DND_MIME)) return
    // The payload isn't readable during dragover, so accept optimistically for
    // any ref drag and validate on drop; keep the cursor a move cursor.
    e.preventDefault()
    e.dataTransfer.dropEffect = 'move'
    if (!dropActive) setDropActive(true)
  }

  const onDragLeave = () => setDropActive(false)

  const onDrop = (e: DragEvent) => {
    setDropActive(false)
    const dragged = readDraggedRef(e)
    if (!dragged || dragged.name === refTag.name) return
    e.preventDefault()
    if (!pairFor(dragged)) return
    // Source = the dragged pill, target = this pill. The modal resolves the
    // direction and the concrete action from these two names.
    openRemoteSync(dragged.name, refTag.name)
  }

  return (
    <RefContextMenu refTag={refTag}>
      <span
        draggable={draggable}
        onDragStart={onDragStart}
        onDragOver={onDragOver}
        onDragLeave={onDragLeave}
        onDrop={onDrop}
        className={cn(
          'max-w-[92px] flex-none overflow-hidden text-ellipsis whitespace-nowrap rounded-[5px] px-1.5 py-px font-mono text-[9.5px] font-semibold leading-[1.4]',
          draggable ? 'cursor-grab active:cursor-grabbing' : 'cursor-default',
          styles[refTag.type],
          dropActive && 'ring-2 ring-primary ring-offset-1 ring-offset-background'
        )}
      >
        {refTag.name}
      </span>
    </RefContextMenu>
  )
}
