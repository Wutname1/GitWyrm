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
  /** The message column fills unused room until the user gives it an explicit width. */
  flexible?: boolean
}

export const COLUMNS: Record<ColumnId, ColumnDef> = {
  refs: { id: 'refs', label: 'BRANCH / TAG', defaultWidth: 150, minWidth: 88, maxWidth: 360 },
  graph: { id: 'graph', label: 'GRAPH', defaultWidth: 124, minWidth: 88, maxWidth: 360 },
  message: { id: 'message', label: 'COMMIT MESSAGE', defaultWidth: 320, minWidth: 160, maxWidth: 720, flexible: true },
  author: { id: 'author', label: 'AUTHOR', defaultWidth: 150, minWidth: 88, maxWidth: 320 },
  changes: { id: 'changes', label: 'CHANGES', defaultWidth: 160, minWidth: 112, maxWidth: 280 },
  date: { id: 'date', label: 'DATE', defaultWidth: 110, minWidth: 88, maxWidth: 220 },
  sha: { id: 'sha', label: 'SHA', defaultWidth: 72, minWidth: 56, maxWidth: 160 },
}

export const DEFAULT_COLUMN_ORDER: ColumnId[] = ['refs', 'graph', 'message', 'author', 'changes', 'date', 'sha']
export type ColumnWidths = Partial<Record<ColumnId, number>>

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

/** Builds the `grid-template-columns` value for the visible columns, in order. */
export function gridTemplate(order: ColumnId[], hidden: ColumnId[], widths: ColumnWidths): string {
  return visibleColumns(order, hidden)
    .map((id) => {
      const column = COLUMNS[id]
      if (column.flexible && widths[id] == null) return `minmax(${column.minWidth}px,1fr)`
      return `${columnWidth(id, widths)}px`
    })
    .join(' ')
}
