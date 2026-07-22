/**
 * Column model for the commit graph. The header, each commit row, the WIP row,
 * and the graph SVG all derive their layout from this single source so columns
 * can be reordered and hidden consistently.
 */

export type ColumnId = 'refs' | 'graph' | 'message' | 'author' | 'changes' | 'date' | 'sha'

export interface ColumnDef {
  id: ColumnId
  label: string
  /** CSS grid track sizing for this column. */
  track: string
  /** Fixed pixel width, used to position the graph SVG. Null for the flexible message column. */
  width: number | null
}

export const COLUMNS: Record<ColumnId, ColumnDef> = {
  refs: { id: 'refs', label: 'BRANCH / TAG', track: '150px', width: 150 },
  graph: { id: 'graph', label: 'GRAPH', track: '124px', width: 124 },
  message: { id: 'message', label: 'COMMIT MESSAGE', track: 'minmax(190px,1fr)', width: null },
  author: { id: 'author', label: 'AUTHOR', track: '150px', width: 150 },
  changes: { id: 'changes', label: 'CHANGES', track: '160px', width: 160 },
  date: { id: 'date', label: 'DATE', track: '110px', width: 110 },
  sha: { id: 'sha', label: 'SHA', track: '72px', width: 72 },
}

export const DEFAULT_COLUMN_ORDER: ColumnId[] = ['refs', 'graph', 'message', 'author', 'changes', 'date', 'sha']

/** Pixel width used for the graph SVG when the GRAPH column is visible. */
export const GRAPH_COLUMN_WIDTH = COLUMNS.graph.width ?? 96

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
export function gridTemplate(order: ColumnId[], hidden: ColumnId[]): string {
  return visibleColumns(order, hidden)
    .map((id) => COLUMNS[id].track)
    .join(' ')
}

/**
 * Left offset (px) where the GRAPH column starts, summing the fixed widths of
 * every visible column before it. Returns null when the graph column is hidden.
 * The message column has no fixed width and never precedes graph in practice,
 * but if it did we cannot resolve a pixel offset, so we treat it as 0.
 */
export function graphLeftOffset(order: ColumnId[], hidden: ColumnId[]): number | null {
  const visible = visibleColumns(order, hidden)
  const graphIndex = visible.indexOf('graph')
  if (graphIndex === -1) return null
  let left = 0
  for (let i = 0; i < graphIndex; i++) {
    left += COLUMNS[visible[i]].width ?? 0
  }
  return left
}
