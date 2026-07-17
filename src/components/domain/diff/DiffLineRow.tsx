import { cn } from '@/lib/utils'
import type { DiffLineEntry } from '@/lib/bindings'

export function DiffLineRow({ line }: { line: DiffLineEntry }) {
  const isHunk = line.sign === '@'
  return (
    <div
      className={cn(
        'flex min-w-max items-baseline',
        isHunk
          ? 'bg-panel2 italic text-muted-foreground'
          : line.sign === '+'
            ? 'bg-added/[.07] text-added'
            : line.sign === '-'
              ? 'bg-removed/[.07] text-removed'
              : 'text-sub'
      )}
    >
      <span className="w-11 flex-none select-none pr-2.5 text-right text-[10px] text-muted-foreground">
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
