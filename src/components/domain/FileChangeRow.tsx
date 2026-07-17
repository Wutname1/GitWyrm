import type { MouseEvent, ReactNode } from 'react'
import { cn } from '@/lib/utils'
import type { FileChange } from '@/lib/bindings'
import { StatusBadge } from './StatusBadge'

interface FileChangeRowProps {
  file: FileChange
  mono?: boolean
  nameClassName?: string
  onOpen: () => void
  action?: ReactNode
}

export function FileChangeRow({ file, mono, nameClassName, onOpen, action }: FileChangeRowProps) {
  return (
    <div
      onClick={onOpen}
      className="flex cursor-pointer items-center gap-2 px-3.5 py-1 hover:bg-panel2"
    >
      <StatusBadge st={file.status} />
      <span
        className={cn(
          'flex-1 overflow-hidden text-ellipsis whitespace-nowrap text-[11.5px]',
          mono && 'font-mono',
          nameClassName ?? 'text-foreground'
        )}
      >
        {file.path}
      </span>
      <span className="font-mono text-[10px] text-added">+{file.additions}</span>
      <span className="font-mono text-[10px] text-removed">−{file.deletions}</span>
      {action}
    </div>
  )
}

interface StageToggleProps {
  direction: 'stage' | 'unstage'
  onToggle: (e: MouseEvent) => void
}

export function StageToggle({ direction, onToggle }: StageToggleProps) {
  return (
    <button
      onClick={onToggle}
      title={direction === 'stage' ? 'Stage' : 'Unstage'}
      className={cn(
        'flex size-[18px] flex-none items-center justify-center rounded border border-border bg-panel2 p-0 text-[13px] leading-none hover:bg-panel3 hover:border-muted-foreground',
        direction === 'stage' ? 'text-added' : 'text-sub'
      )}
    >
      {direction === 'stage' ? '+' : '−'}
    </button>
  )
}
