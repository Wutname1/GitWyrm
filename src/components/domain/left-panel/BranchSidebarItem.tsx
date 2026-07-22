import { type ReactNode } from 'react'
import { cn } from '@/lib/utils'
import type { SectionItem, SidebarSectionData } from '@/lib/types'
import { type DraggedRef } from '@/lib/refSync'
import { useRefDnd } from '@/hooks/useRefDnd'
import { SectionItemRow } from './SectionItemRow'

interface BranchSidebarItemProps {
  section: SidebarSectionData
  item: SectionItem
  currentBranch: string
  isCurrent: boolean
  onClick: () => void
  onDoubleClick?: () => void
  disabled?: boolean
  pending?: boolean
  pendingLabel?: string
  hoverAction?: { icon: ReactNode; title: string; onClick: () => void }
  /** Wraps the row in its right-click menu. */
  renderMenu?: (row: ReactNode) => ReactNode
}

/**
 * A branch row in the sidebar that is also a drag source and drop target, so a
 * branch can be dragged onto another branch (here or a chip in the graph) to
 * sync, merge, or reset -- the same gesture the graph chips support. Uses the
 * shared `useRefDnd` so the pairing rules and highlight match everywhere.
 */
export function BranchSidebarItem({
  section,
  item,
  currentBranch,
  isCurrent,
  onClick,
  onDoubleClick,
  disabled,
  pending,
  pendingLabel,
  hoverAction,
  renderMenu,
}: BranchSidebarItemProps) {
  // The checked-out branch's ref is `head`; every other local branch is `branch`.
  const self: DraggedRef = { name: item.name, type: item.name === currentBranch ? 'head' : 'branch' }
  const dnd = useRefDnd(self)

  const row = (
    <SectionItemRow
      {...dnd.props}
      section={section}
      item={item}
      isCurrent={isCurrent}
      onClick={onClick}
      onDoubleClick={onDoubleClick}
      hoverAction={hoverAction}
      pending={pending}
      disabled={disabled}
      pendingLabel={pendingLabel}
      className={cn(
        dnd.props.draggable && 'wyrm-draggable',
        dnd.isValidTarget && 'wyrm-drop-target',
        dnd.dragging && !dnd.isValidTarget && !dnd.isSource && 'opacity-30'
      )}
    />
  )

  return <>{renderMenu ? renderMenu(row) : row}</>
}
