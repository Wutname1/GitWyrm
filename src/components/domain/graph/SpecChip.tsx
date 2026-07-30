import { Sparkles, Tag } from 'lucide-react'
import { cn } from '@/lib/utils'
import { openSpecDesk } from '@/lib/specDesk'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'

interface SpecChipProps {
  repoId: string
  changeId: string
  /** Task progress, shown only on a linked branch's tip commit. */
  progress?: { done: number; total: number } | null
}

/**
 * The change a commit belongs to, shown on its graph row.
 *
 * On the tip of a linked branch the chip also carries live progress, which is
 * what makes "is this branch done?" answerable without leaving the graph.
 */
export function SpecChip({ repoId, changeId, progress }: SpecChipProps) {
  const showProgress = progress != null && progress.total > 0
  const complete = showProgress && progress.done >= progress.total

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          type="button"
          // The row underneath selects a commit on click; a chip means "go to
          // the change" instead, so it must not also move the selection.
          onClick={(e) => {
            e.stopPropagation()
            void openSpecDesk(repoId, changeId)
          }}
          className={cn(
            'flex max-w-[220px] shrink-0 items-center gap-1 rounded border px-1.5 py-px text-2xs',
            'border-primary/40 bg-soft text-accent-text hover:bg-primary/20',
          )}
        >
          <Tag size={9} className="shrink-0" />
          <span className="truncate font-mono">{changeId}</span>
          {showProgress && (
            <span
              className={cn(
                'shrink-0 font-mono tabular-nums',
                complete ? 'text-accent-text' : 'text-muted-foreground',
              )}
            >
              {progress.done}/{progress.total}
            </span>
          )}
        </button>
      </TooltipTrigger>
      <TooltipContent>
        {showProgress
          ? `${changeId} — ${progress.done} of ${progress.total} tasks done. Click to open the Spec Desk.`
          : `Part of ${changeId}. Click to open the Spec Desk.`}
      </TooltipContent>
    </Tooltip>
  )
}

/**
 * Marks a commit an in-app AI run wrote, so machine work stays distinguishable
 * from the user's own long after the run.
 */
export function AiCommitMarker({ provider }: { provider: string }) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span className="flex shrink-0 items-center text-accent-text">
          <Sparkles size={10} />
        </span>
      </TooltipTrigger>
      <TooltipContent>Written by AI with {provider}, reviewed by you</TooltipContent>
    </Tooltip>
  )
}
