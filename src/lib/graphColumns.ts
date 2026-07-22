/**
 * Column model for the commit graph. The header, each commit row, the WIP row,
 * and the graph SVG all derive their layout from this single source so columns
 * can be reordered and hidden consistently.
 */

export type ColumnId = 'refs' | 'graph' | 'message' | 'author' | 'changes' | 'date' | 'sha'

export interface ColumnDef {
  id: ColumnId
  label: string
  defaultWidth: number
  minWidth: number
  maxWidth: number
  /** The message column grows into unused room but never shrinks below its default. */
  flexible?: boolean
}

export const COLUMNS: Record<ColumnId, ColumnDef> = {
  refs: { id: 'refs', label: 'BRANCH / TAG', defaultWidth: 110, minWidth: 88, maxWidth: 360 },
  graph: { id: 'graph', label: 'GRAPH', defaultWidth: 124, minWidth: 88, maxWidth: 360 },
  message: { id: 'message', label: 'COMMIT MESSAGE', defaultWidth: 380, minWidth: 160, maxWidth: 720, flexible: true },
  author: { id: 'author', label: 'AUTHOR', defaultWidth: 140, minWidth: 34, maxWidth: 320 },
  changes: { id: 'changes', label: 'CHANGES', defaultWidth: 160, minWidth: 112, maxWidth: 280 },
  date: { id: 'date', label: 'DATE', defaultWidth: 110, minWidth: 88, maxWidth: 220 },
  sha: { id: 'sha', label: 'SHA', defaultWidth: 72, minWidth: 56, maxWidth: 160 },
}

export const DEFAULT_COLUMN_ORDER: ColumnId[] = ['refs', 'graph', 'message', 'author', 'changes', 'date', 'sha']
export type ColumnWidths = Partial<Record<ColumnId, number>>

/**
 * Below this width the author column drops its name and shows just the avatar,
 * the same way the repository tabs collapse to icons. Dragging the handle past
 * the threshold snaps to the icon-only width rather than leaving a clipped name.
 */
export const AUTHOR_ICON_ONLY_WIDTH = COLUMNS.author.minWidth
const AUTHOR_COLLAPSE_AT = 96

export function isAuthorIconOnly(widths: ColumnWidths): boolean {
  return columnWidth('author', widths) < AUTHOR_COLLAPSE_AT
}

/** Snaps an in-progress author resize to icon-only once it passes the threshold. */
export function snapAuthorWidth(width: number): number {
  return width < AUTHOR_COLLAPSE_AT ? AUTHOR_ICON_ONLY_WIDTH : width
}

export function clampColumnWidth(id: ColumnId, width: number): number {
  const column = COLUMNS[id]
  if (!Number.isFinite(width)) return column.defaultWidth
  return Math.round(Math.min(column.maxWidth, Math.max(column.minWidth, width)))
}

export function columnWidth(id: ColumnId, widths: ColumnWidths): number {
  return clampColumnWidth(id, widths[id] ?? COLUMNS[id].defaultWidth)
}

export function normalizeColumnWidths(
  widths: Partial<Record<string, number>> | undefined,
): ColumnWidths {
  const result: ColumnWidths = {}
  for (const id of DEFAULT_COLUMN_ORDER) {
    const width = widths?.[id]
    if (width != null && Number.isFinite(width)) result[id] = clampColumnWidth(id, width)
  }
  return result
}

/** Columns visible, in display order (order minus hidden). */
export function visibleColumns(order: ColumnId[], hidden: ColumnId[]): ColumnId[] {
  const hiddenSet = new Set(hidden)
  return order.filter((id) => !hiddenSet.has(id))
}

/**
 * Applies the change-size display setting without overwriting the user's saved
 * column layout. The Changes column only exists while column mode is active.
 */
export function effectiveHiddenColumns(
  hidden: ColumnId[],
  showChangeIndicator: boolean,
  changeSizeDisplay: 'row' | 'column',
): ColumnId[] {
  if (showChangeIndicator && changeSizeDisplay === 'column') return hidden
  return hidden.includes('changes') ? hidden : [...hidden, 'changes']
}

/**
 * Builds the `grid-template-columns` value for the visible columns, in order.
 *
 * Columns keep their configured width even when the total overflows the viewport,
 * so the graph scrolls sideways instead of squeezing every column to fit. The
 * flexible column still absorbs leftover room when there is any, but its default
 * width is the floor rather than its minimum.
 */
export function gridTemplate(order: ColumnId[], hidden: ColumnId[], widths: ColumnWidths): string {
  return visibleColumns(order, hidden)
    .map((id) => {
      const column = COLUMNS[id]
      if (column.flexible && widths[id] == null) return `minmax(${column.defaultWidth}px,1fr)`
      return `${columnWidth(id, widths)}px`
    })
    .join(' ')
}

/**
 * Sum of the visible columns at their configured widths. The header and the
 * scrolling row area both use this as a min-width so their tracks stay aligned
 * once the content is wider than the viewport.
 */
export function totalColumnsWidth(order: ColumnId[], hidden: ColumnId[], widths: ColumnWidths): number {
  return visibleColumns(order, hidden).reduce((total, id) => {
    const column = COLUMNS[id]
    if (column.flexible && widths[id] == null) return total + column.defaultWidth
    return total + columnWidth(id, widths)
  }, 0)
}
