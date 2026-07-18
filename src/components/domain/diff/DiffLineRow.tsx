import type { ReactNode } from 'react'
import { cn } from '@/lib/utils'
import type { DiffLineEntry } from '@/lib/bindings'

interface DiffLineRowProps {
  line: DiffLineEntry
  selectable?: boolean
  /** Left-click selected — the fully Active state. */
  selected?: boolean
  /** This line's right-click menu is open — the Semi-Active (target) state. */
  contextActive?: boolean
  onSelect?: (shift: boolean) => void
  onContextMenu?: (e: React.MouseEvent) => void
  children?: ReactNode
}

export function DiffLineRow({
  line,
  selectable,
  selected,
  contextActive,
  onSelect,
  onContextMenu,
  children,
}: DiffLineRowProps) {
  const isHunk = line.sign === '@'
  return (
    <div
      onClick={selectable ? (e) => onSelect?.(e.shiftKey) : undefined}
      onContextMenu={onContextMenu}
      data-selected={selected ? '' : undefined}
      data-context-active={contextActive ? '' : undefined}
      className={cn(
        'group/line relative flex min-w-max items-baseline outline-offset-[-1px] transition-colors duration-75',
        selectable && 'cursor-pointer',
        // Base tint by line kind (Rest state).
        isHunk
          ? 'bg-panel2 italic text-muted-foreground'
          : line.sign === '+'
            ? 'bg-added/[.07] text-added'
            : line.sign === '-'
              ? 'bg-removed/[.07] text-removed'
              : 'text-sub',
        // Mouseover: a clearly brighter overlay than rest, plus a faint left
        // marker. Only for selectable lines, and only when not in a stronger
        // state. Uses a bright white wash so it reads on colored +/- lines too.
        selectable &&
          !selected &&
          !contextActive &&
          'hover:bg-[rgba(255,255,255,0.1)] hover:shadow-[inset_2px_0_0_var(--gw-sub)]',
        // Semi-Active: right-click target — dashed accent outline + light accent
        // fill, distinct from the solid Active fill so the two never look alike.
        contextActive &&
          !selected &&
          'bg-primary/15 outline outline-1 outline-dashed outline-primary/70',
        // Active: left-click selected — strong solid fill + solid accent ring.
        selected && 'bg-primary/25 shadow-[inset_2px_0_0_var(--gw-accent)] ring-1 ring-inset ring-primary/60'
      )}
    >
      <span
        className={cn(
          'w-4 flex-none select-none text-center text-[10px]',
          selected ? 'text-primary' : contextActive ? 'text-primary/70' : 'text-transparent'
        )}
      >
        {selectable ? (selected ? '✓' : '·') : ''}
      </span>
      <span className="w-10 flex-none select-none pr-2.5 text-right text-[10px] text-muted-foreground">
        {isHunk ? '' : (line.new_no ?? line.old_no ?? '')}
      </span>
      <span
        className={cn(
          'w-4 flex-none select-none',
          line.sign === '+' ? 'text-added' : line.sign === '-' ? 'text-removed' : 'text-muted-foreground'
        )}
      >
        {line.sign === '-' ? '−' : isHunk ? '' : line.sign}
      </span>
      <span className="whitespace-pre pr-5">{line.text}</span>
      {children}
    </div>
  )
}
