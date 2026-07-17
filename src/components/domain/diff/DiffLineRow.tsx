import { cn } from '@/lib/utils'
import type { DiffLineEntry } from '@/lib/bindings'

interface DiffLineRowProps {
  line: DiffLineEntry
  selectable?: boolean
  selected?: boolean
  onSelect?: (shift: boolean) => void
}

export function DiffLineRow({ line, selectable, selected, onSelect }: DiffLineRowProps) {
  const isHunk = line.sign === '@'
  return (
    <div
      onClick={selectable ? (e) => onSelect?.(e.shiftKey) : undefined}
      className={cn(
        'flex min-w-max items-baseline',
        selectable && 'cursor-pointer',
        isHunk
          ? 'bg-panel2 italic text-muted-foreground'
          : line.sign === '+'
            ? 'bg-added/[.07] text-added'
            : line.sign === '-'
              ? 'bg-removed/[.07] text-removed'
              : 'text-sub',
        selected && 'bg-primary/20 ring-1 ring-inset ring-primary/50'
      )}
    >
      <span
        className={cn(
          'w-4 flex-none select-none text-center text-[10px]',
          selected ? 'text-primary' : 'text-transparent'
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
    </div>
  )
}
