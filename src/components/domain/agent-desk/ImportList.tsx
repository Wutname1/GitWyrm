import { useRef } from 'react'
import { useVirtualizer } from '@tanstack/react-virtual'
import { Check, ChevronDown, ChevronRight, Minus } from 'lucide-react'
import type { ImportRow, SelectModifiers } from '@/lib/agentImportList'
import { projectFilterName } from '@/lib/agentImportList'
import { cn } from '@/lib/utils'

/**
 * The virtualized chat list for "Import chats".
 *
 * A sibling of `VirtualSessionList` rather than a reuse of it: that component
 * is typed to `SidebarRow`/`AgentSessionHeader` and renders GitWyrm's own
 * chats with rename/archive/delete. These rows are external chats with
 * checkboxes and a bulk action. Both use `@tanstack/react-virtual` the same
 * way, so there is still exactly one virtualization approach in the app.
 *
 * Rows issue no queries of their own. That is the whole point of this file
 * existing: the previous list mounted three mutation hooks and a per-row
 * capability probe on every row, so a Claude Code install with hundreds of
 * saved chats fired hundreds of file reads on open. A row here is a pure
 * function of props.
 */

const ROW_HEIGHT = 46
const HEADER_HEIGHT = 26

export function ImportList({
  rows,
  selected,
  activeId,
  onSelect,
  onToggleGroup,
  onOpenDetails,
}: {
  rows: ImportRow[]
  selected: ReadonlySet<string>
  /** The one chat whose details are open, if any. */
  activeId: string | null
  onSelect: (id: string, mods: SelectModifiers) => void
  onToggleGroup: (groupId: string) => void
  onOpenDetails: (id: string) => void
}) {
  const scrollRef = useRef<HTMLDivElement>(null)

  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: (i) => (rows[i]?.kind === 'header' ? HEADER_HEIGHT : ROW_HEIGHT),
    overscan: 12,
  })

  return (
    <div ref={scrollRef} className="min-h-0 flex-1 overflow-y-auto">
      <div style={{ height: virtualizer.getTotalSize(), position: 'relative' }}>
        {virtualizer.getVirtualItems().map((vi) => {
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
                  'flex w-full items-center gap-1 px-2 text-left',
                  'text-2xs font-bold uppercase tracking-wide text-muted-foreground',
                  'hover:text-foreground'
                )}
              >
                {row.collapsed ? <ChevronRight size={11} /> : <ChevronDown size={11} />}
                <span className="min-w-0 flex-1 truncate">{row.label}</span>
                <span className="flex-none font-mono text-2xs normal-case">{row.count}</span>
              </button>
            )
          }

          return (
            <ImportSessionRow
              key={row.id}
              row={row}
              style={style}
              checked={selected.has(row.id)}
              active={activeId === row.id}
              onSelect={onSelect}
              onOpenDetails={onOpenDetails}
            />
          )
        })}
      </div>
    </div>
  )
}

function ImportSessionRow({
  row,
  style,
  checked,
  active,
  onSelect,
  onOpenDetails,
}: {
  row: Extract<ImportRow, { kind: 'session' }>
  style: React.CSSProperties
  checked: boolean
  active: boolean
  onSelect: (id: string, mods: SelectModifiers) => void
  onOpenDetails: (id: string) => void
}) {
  const { session } = row
  const imported = session.importedSessionId != null
  const unresolved = session.project.kind === 'unresolved'
  const project = projectFilterName(session)

  return (
    <div
      style={style}
      // The row itself is the click target for selection, so hitting the 12px
      // checkbox is never required -- the same reach the commit list gives.
      onClick={(e) => onSelect(row.id, { shift: e.shiftKey, ctrl: e.ctrlKey || e.metaKey })}
      onDoubleClick={() => onOpenDetails(row.id)}
      role="option"
      aria-selected={checked}
      tabIndex={0}
      onKeyDown={(e) => {
        if (e.key === ' ' || e.key === 'Enter') {
          e.preventDefault()
          onSelect(row.id, { shift: e.shiftKey, ctrl: e.ctrlKey || e.metaKey })
        }
      }}
      className={cn(
        'flex cursor-pointer items-center gap-2 px-2 outline-none transition-colors',
        'hover:bg-panel2 focus-visible:ring-[3px] focus-visible:ring-primary/40',
        // A tint alone is not a selected state (DESIGN.md). The mint edge is
        // what makes a checked row read as checked at a glance down a long
        // list, which is exactly when it matters.
        checked && 'bg-soft shadow-[inset_2px_0_0_var(--gw-accent)]',
        active && !checked && 'bg-panel2'
      )}
    >
      <span
        aria-hidden
        className={cn(
          'flex size-3.5 flex-none items-center justify-center rounded-[3px] border transition-colors',
          checked
            ? 'border-primary bg-primary text-primary-foreground'
            : 'border-border bg-transparent'
        )}
      >
        {checked && <Check size={10} strokeWidth={3} />}
      </span>

      <span className="flex min-w-0 flex-1 flex-col justify-center">
        <span className="truncate text-xs text-foreground" title={session.summary.title}>
          {session.summary.title}
        </span>
        <span
          className={cn(
            'truncate text-2xs',
            unresolved ? 'text-[var(--gw-amber)]' : 'text-muted-foreground'
          )}
          title={project}
        >
          {unresolved ? `Folder not found: ${project}` : project}
        </span>
      </span>

      <span className="flex flex-none items-center gap-1.5">
        <span className="font-mono text-2xs text-muted-foreground">
          {session.summary.messageCount}
        </span>
        {imported && (
          // Says it is already here, so a long select-all is not read as
          // hundreds of duplicates about to be made.
          <span className="inline-flex items-center gap-0.5 rounded-[3px] bg-panel3 px-1 py-px text-2xs font-semibold uppercase tracking-wide text-muted-foreground">
            <Minus size={8} aria-hidden />
            In
          </span>
        )}
      </span>
    </div>
  )
}
