import { useState } from 'react'
import { useWorkspaceStore } from '@/stores/workspaceStore'
import {
  COLUMNS,
  DEFAULT_COLUMN_ORDER,
  gridTemplate,
  visibleColumns,
  type ColumnId,
} from '@/lib/graphColumns'
import {
  ContextMenu,
  ContextMenuCheckboxItem,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from '@/components/ui/context-menu'
import { cn } from '@/lib/utils'
import { TooltipHint } from '@/components/ui/tooltip'

/**
 * Commit-graph column header. Right-click any header cell for a menu to show or
 * hide columns; drag a header onto another to reorder. Both write through to the
 * persisted layout in the workspace store.
 */
export function GraphHeader() {
  const order = useWorkspaceStore((s) => s.columnOrder)
  const hidden = useWorkspaceStore((s) => s.hiddenColumns)
  const toggleColumn = useWorkspaceStore((s) => s.toggleColumn)
  const reorderColumn = useWorkspaceStore((s) => s.reorderColumn)
  const resetColumns = useWorkspaceStore((s) => s.resetColumns)

  const [dragId, setDragId] = useState<ColumnId | null>(null)
  const [overId, setOverId] = useState<ColumnId | null>(null)

  const visible = visibleColumns(order, hidden)
  const hiddenSet = new Set(hidden)
  const isModified = hidden.length > 0 || order.some((id, i) => id !== DEFAULT_COLUMN_ORDER[i])

  const handleDrop = (targetId: ColumnId) => {
    if (dragId && dragId !== targetId) {
      // Drop lands the dragged column at the target's current display position.
      reorderColumn(dragId, order.indexOf(targetId))
    }
    setDragId(null)
    setOverId(null)
  }

  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>
        <div
          className={cn(
            'grid h-[30px] flex-none items-center border-b border-border pl-3 pr-1 text-[10px] font-bold tracking-[.06em] text-muted-foreground',
          )}
          style={{ gridTemplateColumns: gridTemplate(order, hidden) }}
        >
          {visible.map((id) => (
            <TooltipHint
              key={id}
              label="Drag to reorder. Right-click to show or hide columns."
            >
              <span
                draggable
                onDragStart={() => setDragId(id)}
                onDragEnd={() => {
                  setDragId(null)
                  setOverId(null)
                }}
                onDragOver={(e) => {
                  e.preventDefault()
                  if (id !== overId) setOverId(id)
                }}
                onDrop={(e) => {
                  e.preventDefault()
                  handleDrop(id)
                }}
                className={cn(
                  'flex h-full cursor-grab select-none items-center active:cursor-grabbing',
                  dragId === id && 'opacity-40',
                  overId === id && dragId && dragId !== id && 'bg-soft shadow-[inset_2px_0_0_var(--gw-accent)]'
                )}
              >
                {COLUMNS[id].label}
              </span>
            </TooltipHint>
          ))}
        </div>
      </ContextMenuTrigger>
      <ContextMenuContent className="w-52">
        {DEFAULT_COLUMN_ORDER.map((id) => {
          const isVisible = !hiddenSet.has(id)
          // Never let the user hide the last remaining column.
          const isLastVisible = isVisible && visible.length === 1
          return (
            <ContextMenuCheckboxItem
              key={id}
              checked={isVisible}
              disabled={isLastVisible}
              onSelect={(e) => {
                e.preventDefault()
                if (!isLastVisible) toggleColumn(id)
              }}
            >
              {COLUMNS[id].label}
            </ContextMenuCheckboxItem>
          )
        })}
        <ContextMenuSeparator />
        <ContextMenuItem disabled={!isModified} onSelect={() => resetColumns()}>
          Reset columns
        </ContextMenuItem>
      </ContextMenuContent>
    </ContextMenu>
  )
}
