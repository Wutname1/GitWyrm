import type { MouseEvent, ReactNode } from 'react'
import { Package } from 'lucide-react'
import { cn } from '@/lib/utils'
import type { FileChange } from '@/lib/bindings'
import { StatusBadge } from './StatusBadge'
import { FileChangeMenu } from './commit-form/FileChangeMenu'
import { PendingIndicator } from '@/components/ui/pending-indicator'

/** Plain-language note for a moved submodule pointer, e.g. "5 commits ahead". */
function submoduleNote(sub: NonNullable<FileChange['submodule']>): string {
  if (!sub.initialized) return 'not downloaded yet'
  if (sub.ahead > 0 && sub.behind === 0)
    return `${sub.ahead} commit${sub.ahead === 1 ? '' : 's'} ahead`
  if (sub.behind > 0 && sub.ahead === 0)
    return `${sub.behind} commit${sub.behind === 1 ? '' : 's'} behind`
  return 'points to a different commit'
}

interface FileChangeRowProps {
  file: FileChange
  mono?: boolean
  nameClassName?: string
  onOpen: () => void
  action?: ReactNode
  /**
   * When set, wraps the row in a right-click menu with stage/discard actions.
   * `true` marks the file as staged, `false` as unstaged. Omit for read-only
   * rows (e.g. a past commit's files) that should have no menu.
   */
  menuStaged?: boolean
}

export function FileChangeRow({
  file,
  mono,
  nameClassName,
  onOpen,
  action,
  menuStaged,
}: FileChangeRowProps) {
  const sub = file.submodule
  const row = (
    <div
      onClick={onOpen}
      className="flex cursor-pointer items-center gap-2 px-3.5 py-1 hover:bg-panel2"
    >
      <StatusBadge st={file.status} />
      <span
        className={cn(
          'flex flex-1 items-center gap-1.5 overflow-hidden text-ellipsis whitespace-nowrap text-[11.5px]',
          mono && 'font-mono',
          nameClassName ?? 'text-foreground'
        )}
      >
        {sub && <Package className="size-3 flex-none text-sub" aria-label="Submodule" />}
        <span className="overflow-hidden text-ellipsis">{file.path}</span>
      </span>
      {sub ? (
        // Line counts are meaningless for a submodule pointer; show what moved.
        <span className="whitespace-nowrap text-[10px] text-sub">submodule · {submoduleNote(sub)}</span>
      ) : (
        <>
          <span className="font-mono text-[10px] text-added">+{file.additions}</span>
          <span className="font-mono text-[10px] text-removed">−{file.deletions}</span>
        </>
      )}
      {action}
    </div>
  )

  if (menuStaged === undefined) return row

  return (
    <FileChangeMenu file={file} staged={menuStaged} onOpen={onOpen}>
      {row}
    </FileChangeMenu>
  )
}

interface StageToggleProps {
  direction: 'stage' | 'unstage'
  onToggle: (e: MouseEvent) => void
  disabled?: boolean
  pending?: boolean
}

export function StageToggle({ direction, onToggle, disabled, pending }: StageToggleProps) {
  return (
    <button
      onClick={onToggle}
      disabled={disabled}
      aria-busy={pending || undefined}
      title={pending ? (direction === 'stage' ? 'Staging file' : 'Unstaging file') : direction === 'stage' ? 'Stage' : 'Unstage'}
      className={cn(
        'flex size-[18px] flex-none items-center justify-center rounded border border-border bg-panel2 p-0 text-[13px] leading-none hover:border-muted-foreground hover:bg-panel3 disabled:pointer-events-none disabled:opacity-40',
        direction === 'stage' ? 'text-added' : 'text-sub',
        pending && 'border-primary/50 bg-soft !text-primary !opacity-100'
      )}
    >
      {pending ? <PendingIndicator className="size-3" /> : direction === 'stage' ? '+' : '−'}
    </button>
  )
}
