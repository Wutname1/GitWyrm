import type { BranchInfo } from '@/lib/bindings'
import { cn } from '@/lib/utils'
import { branchSync } from '@/lib/branchActions'

interface SyncBadgeProps {
  branch: BranchInfo
  className?: string
}

/**
 * How far a branch is from its remote, as a short badge: `↑2 ↓1` when it has
 * diverged, `new` when it has never been sent, `gone` when the branch it
 * tracked is no longer there. Renders nothing when the branch matches.
 *
 * Every surface that shows this reads it from here, so a branch cannot look
 * synced in one place and unsynced in another.
 */
export function SyncBadge({ branch, className }: SyncBadgeProps) {
  const sync = branchSync(branch)
  if (!sync.text) return null

  return (
    <span
      title={sync.title ?? undefined}
      className={cn(
        'whitespace-nowrap font-mono text-2xs',
        sync.marker
          ? 'font-semibold uppercase tracking-wide text-accent-text/80'
          : 'text-muted-foreground',
        className
      )}
    >
      {sync.text}
    </span>
  )
}
