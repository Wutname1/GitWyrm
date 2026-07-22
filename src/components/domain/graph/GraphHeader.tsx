import { useState } from "react";
import { useWorkspaceStore } from "@/stores/workspaceStore";
import {
  COLUMNS,
  DEFAULT_COLUMN_ORDER,
  effectiveHiddenColumns,
  gridTemplate,
  isAuthorIconOnly,
  snapAuthorWidth,
  totalColumnsWidth,
  visibleColumns,
  type ColumnId,
} from "@/lib/graphColumns";
import {
  ContextMenu,
  ContextMenuCheckboxItem,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import { cn } from "@/lib/utils";
import { ResizeHandle } from "@/components/ui/ResizeHandle";

/**
 * Commit-graph column header. Right-click any header cell for a menu to show or
 * hide columns; drag a header onto another to reorder. Both write through to the
 * persisted layout in the workspace store.
 *
 * `scrollRef` is driven by the graph body's horizontal scroll so the headers
 * stay lined up with their columns while the header itself stays pinned.
 */
export function GraphHeader({ scrollRef }: { scrollRef?: React.Ref<HTMLDivElement> }) {
  const order = useWorkspaceStore((s) => s.columnOrder);
  const hidden = useWorkspaceStore((s) => s.hiddenColumns);
  const widths = useWorkspaceStore((s) => s.columnWidths);
  const changeSizeDisplay = useWorkspaceStore((s) => s.changeSizeDisplay);
  const showChangeIndicator = useWorkspaceStore((s) => s.showChangeIndicator);
  const toggleColumn = useWorkspaceStore((s) => s.toggleColumn);
  const reorderColumn = useWorkspaceStore((s) => s.reorderColumn);
  const resetColumns = useWorkspaceStore((s) => s.resetColumns);
  const setColumnWidth = useWorkspaceStore((s) => s.setColumnWidth);
  const resetColumnWidth = useWorkspaceStore((s) => s.resetColumnWidth);
  const setChangeSizeDisplay = useWorkspaceStore((s) => s.setChangeSizeDisplay);
  const setShowChangeIndicator = useWorkspaceStore((s) => s.setShowChangeIndicator);

  const [dragId, setDragId] = useState<ColumnId | null>(null);
  const [overId, setOverId] = useState<ColumnId | null>(null);

  const effectiveHidden = effectiveHiddenColumns(hidden, showChangeIndicator, changeSizeDisplay);
  const visible = visibleColumns(order, effectiveHidden);
  const authorIconOnly = isAuthorIconOnly(widths);
  const hiddenSet = new Set(hidden);
  const isModified =
    hidden.length > 0 ||
    Object.keys(widths).length > 0 ||
    order.some((id, i) => id !== DEFAULT_COLUMN_ORDER[i]);

  const handleDrop = (targetId: ColumnId) => {
    if (dragId && dragId !== targetId) {
      // Drop lands the dragged column at the target's current display position.
      reorderColumn(dragId, order.indexOf(targetId));
    }
    setDragId(null);
    setOverId(null);
  };

  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>
        <div
          ref={scrollRef}
          className={cn(
            "h-[30px] flex-none overflow-hidden border-b border-border pl-3 pr-1 text-2xs font-bold tracking-[.06em] text-muted-foreground",
          )}
        >
          <div
            className="grid h-full items-center"
            style={{
              gridTemplateColumns: gridTemplate(order, effectiveHidden, widths),
              minWidth: totalColumnsWidth(order, effectiveHidden, widths),
            }}
          >
            {visible.map((id) => (
              <span
                key={id}
                draggable
                onDragStart={() => setDragId(id)}
                onDragEnd={() => {
                  setDragId(null);
                  setOverId(null);
                }}
                onDragOver={(e) => {
                  e.preventDefault();
                  if (id !== overId) setOverId(id);
                }}
                onDrop={(e) => {
                  e.preventDefault();
                  handleDrop(id);
                }}
                className={cn(
                  "relative flex h-full cursor-grab select-none items-center pr-2 active:cursor-grabbing",
                  dragId === id && "opacity-40",
                  overId === id &&
                    dragId &&
                    dragId !== id &&
                    "bg-soft shadow-[inset_2px_0_0_var(--gw-accent)]",
                )}
              >
                <span className="min-w-0 overflow-hidden text-ellipsis whitespace-nowrap">
                  {id === 'author' && authorIconOnly ? '' : COLUMNS[id].label}
                </span>
                <ResizeHandle
                  ariaLabel={`Resize ${COLUMNS[id].label.toLowerCase()} column`}
                  value={widths[id]}
                  min={COLUMNS[id].minWidth}
                  max={COLUMNS[id].maxWidth}
                  defaultValue={COLUMNS[id].defaultWidth}
                  getCurrentValue={(handle) =>
                    handle.parentElement?.getBoundingClientRect().width ?? COLUMNS[id].defaultWidth
                  }
                  onChange={(width) =>
                    setColumnWidth(id, id === 'author' ? snapAuthorWidth(width) : width)
                  }
                  onReset={() => resetColumnWidth(id)}
                  className="-right-1"
                />
              </span>
            ))}
          </div>
        </div>
      </ContextMenuTrigger>
      <ContextMenuContent className="w-52">
        {DEFAULT_COLUMN_ORDER.map((id) => {
          const isChanges = id === 'changes';
          const isVisible = isChanges ? !effectiveHidden.includes(id) : !hiddenSet.has(id);
          // Never let the user hide the last remaining column.
          const isLastVisible = isVisible && visible.length === 1;
          return (
            <ContextMenuCheckboxItem
              key={id}
              checked={isVisible}
              disabled={isLastVisible}
              onSelect={(e) => {
                e.preventDefault();
                if (isLastVisible) return;
                if (!isChanges) {
                  toggleColumn(id);
                  return;
                }
                if (isVisible) {
                  setShowChangeIndicator(false);
                  return;
                }
                if (hiddenSet.has(id)) toggleColumn(id);
                setChangeSizeDisplay('column');
                setShowChangeIndicator(true);
              }}
            >
              {COLUMNS[id].label}
            </ContextMenuCheckboxItem>
          );
        })}
        <ContextMenuSeparator />
        <ContextMenuItem disabled={!isModified} onSelect={() => resetColumns()}>
          Reset columns
        </ContextMenuItem>
      </ContextMenuContent>
    </ContextMenu>
  );
}
