import { useRef } from 'react'
import { useVirtualizer } from '@tanstack/react-virtual'
import { ChevronDown, ChevronRight } from 'lucide-react'
import type { SidebarRow } from '@/lib/agentSessionGrouping'
import { SESSION_ROW_HEIGHT, SessionRow } from '@/components/domain/agent-desk/SessionRow'
import { cn } from '@/lib/utils'

const GROUP_HEADER_HEIGHT = 24

/**
 * Virtualized rendering of the flat `SidebarRow[]` grouping produces.
 *
 * Uses `@tanstack/react-virtual` the same way `GraphView.tsx` does (already a
 * dependency; no new one added), so dozens-to-thousands of sessions cost the
 * same DOM regardless of how many are loaded -- task 3.1/3.7's "1,000 rows"
 * requirement is about this component staying responsive, which the pure
 * `buildSidebarRows` unit tests cover for correctness and this covers for
 * rendering cost (verified in-app, not by a DOM test -- see report).
 */
export function VirtualSessionList({
  rows,
  selectedId,
  onSelectSession,
  onRename,
  onArchive,
  onDelete,
  onToggleGroup,
  emptyMessage,
}: {
  rows: SidebarRow[]
  selectedId: string | null
  onSelectSession: (sessionId: string) => void
  onRename: (sessionId: string, title: string) => void
  onArchive: (sessionId: string, archived: boolean) => void
  onDelete: (sessionId: string, title: string) => void
  onToggleGroup: (groupId: string) => void
  emptyMessage: React.ReactNode
}) {
  const scrollRef = useRef<HTMLDivElement>(null)

  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: (i) => (rows[i]?.kind === 'header' ? GROUP_HEADER_HEIGHT : SESSION_ROW_HEIGHT),
    overscan: 12,
  })

  if (rows.length === 0) {
    return (
      <div className="flex flex-1 items-center justify-center p-4 text-center">
        <p className="max-w-[14rem] text-2xs leading-relaxed text-muted-foreground">{emptyMessage}</p>
      </div>
    )
  }

  const items = virtualizer.getVirtualItems()

  return (
    <div ref={scrollRef} className="min-h-0 flex-1 overflow-y-auto px-1.5">
      <div style={{ height: virtualizer.getTotalSize(), position: 'relative' }}>
        {items.map((vi) => {
          const row = rows[vi.index]
          if (!row) return null

          const style: React.CSSProperties = {
            position: 'absolute',
            top: 0,
            left: 0,
            right: 0,
            transform: `translateY(${vi.start}px)`,
            height: vi.size,
          }

          if (row.kind === 'header') {
            return (
              <button
                key={row.id}
                type="button"
                style={style}
                onClick={() => onToggleGroup(row.id)}
                aria-expanded={!row.collapsed}
                className={cn(
                  'flex w-full items-center gap-1 px-1.5 text-left',
                  'text-[10px] font-bold uppercase tracking-wide text-muted-foreground',
                  'hover:text-foreground'
                )}
              >
                {row.collapsed ? <ChevronRight size={11} /> : <ChevronDown size={11} />}
                <span className="min-w-0 flex-1 truncate">{row.label}</span>
                <span className="flex-none font-mono text-[9px] normal-case text-muted-foreground">
                  {row.count}
                </span>
              </button>
            )
          }

          return (
            <SessionRow
              key={row.id}
              header={row.header}
              showProject={!row.projectInHeader}
              selected={row.id === selectedId}
              onSelect={onSelectSession}
              onRename={onRename}
              onArchive={onArchive}
              onDelete={onDelete}
              style={style}
            />
          )
        })}
      </div>
    </div>
  )
}
