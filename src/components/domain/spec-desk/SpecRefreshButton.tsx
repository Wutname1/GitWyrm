import { useState } from 'react'
import { RefreshCw } from 'lucide-react'
import { useQueryClient } from '@tanstack/react-query'
import { cn } from '@/lib/utils'
import { refreshSpecsEverywhere } from '@/lib/specSync'

/**
 * Re-read this repo's spec files from disk, in every window.
 *
 * Exists because the `openspec/` folder is not only written by GitWyrm: undoing
 * a commit, switching branches, or an agent editing tasks.md all move the files
 * underneath a list that was read minutes ago. There is a watcher, but it cannot
 * cover every case -- a rewind replaces the whole folder at once -- so the user
 * needs a way to say "look again" without closing the window.
 *
 * The spin is deliberately held for a beat past the refetch. Re-reading a
 * handful of small markdown files is usually faster than a frame, and a button
 * that does nothing visible reads as a broken button.
 */
export function SpecRefreshButton({
  repoId,
  className,
  variant = 'icon',
}: {
  repoId: string
  className?: string
  /** `icon` for a toolbar; `label` where there is room for the word. */
  variant?: 'icon' | 'label'
}) {
  const qc = useQueryClient()
  const [spinning, setSpinning] = useState(false)

  const refresh = () => {
    refreshSpecsEverywhere(qc, repoId)
    setSpinning(true)
    window.setTimeout(() => setSpinning(false), 550)
  }

  return (
    <button
      type="button"
      onClick={refresh}
      title="Re-read the spec files from disk"
      aria-label="Refresh specs"
      className={cn(
        'flex flex-none items-center gap-1.5 rounded text-muted-foreground transition-colors hover:text-foreground',
        variant === 'icon' ? 'p-1 hover:bg-panel2' : 'px-1.5 py-1 text-2xs hover:bg-panel2',
        className
      )}
    >
      <RefreshCw
        size={variant === 'icon' ? 12 : 11}
        strokeWidth={2.2}
        className={cn('flex-none', spinning && 'animate-spin')}
      />
      {variant === 'label' && <span>Refresh</span>}
    </button>
  )
}
