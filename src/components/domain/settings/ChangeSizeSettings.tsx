import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'
import { useWorkspaceStore } from '@/stores/workspaceStore'
import { ChangeSizeIndicator } from '../graph/ChangeSizeIndicator'

export function ChangeSizeSettings() {
  const display = useWorkspaceStore((s) => s.changeSizeDisplay)
  const showIndicator = useWorkspaceStore((s) => s.showChangeIndicator)
  const showLineCounts = useWorkspaceStore((s) => s.showChangeLineCounts)
  const setDisplay = useWorkspaceStore((s) => s.setChangeSizeDisplay)
  const setShowIndicator = useWorkspaceStore((s) => s.setShowChangeIndicator)
  const setShowLineCounts = useWorkspaceStore((s) => s.setShowChangeLineCounts)

  return (
    <div className="flex max-w-xl flex-col gap-3">
      <label className="flex cursor-pointer items-center gap-2 text-xs text-foreground">
        <input
          type="checkbox"
          checked={showIndicator}
          onChange={(event) => setShowIndicator(event.target.checked)}
          className="size-3.5 accent-[var(--gw-accent)]"
        />
        Show change size in the commit graph
      </label>

      <div className="flex items-center gap-2">
        <span className="w-20 flex-none text-2xs text-sub">Display</span>
        <div className="inline-flex rounded-md border border-border bg-background p-0.5" role="group" aria-label="Change size display">
          {(['column', 'row'] as const).map((option) => (
            <Button
              key={option}
              type="button"
              variant="ghost"
              size="xs"
              aria-pressed={display === option}
              disabled={!showIndicator}
              onClick={() => setDisplay(option)}
              className={cn(
                'min-w-16 capitalize text-sub',
                display === option && 'bg-panel3 text-foreground shadow-sm',
              )}
            >
              {option}
            </Button>
          ))}
        </div>
      </div>

      <label className={cn(
        'flex items-center gap-2 text-xs text-foreground',
        showIndicator ? 'cursor-pointer' : 'cursor-not-allowed opacity-50',
      )}>
        <input
          type="checkbox"
          checked={showLineCounts}
          disabled={!showIndicator}
          onChange={(event) => setShowLineCounts(event.target.checked)}
          className="size-3.5 accent-[var(--gw-accent)]"
        />
        Show exact added and removed lines
      </label>

      <div className="rounded-md border border-border bg-background p-3">
        <div className="mb-2 text-2xs font-semibold uppercase tracking-[.05em] text-muted-foreground">
          Live example
        </div>
        <div className={cn(
          'min-w-0 rounded border border-border bg-panel px-2.5',
          display === 'row' ? 'flex h-[42px] flex-col justify-center' : 'flex h-8 items-center gap-4',
        )}>
          <span className="min-w-0 flex-1 overflow-hidden text-ellipsis whitespace-nowrap text-xs text-foreground">
            Improve changed file view
          </span>
          {showIndicator && (
            <ChangeSizeIndicator
              filesChanged={6}
              additions={84}
              deletions={21}
              showLineCounts={showLineCounts}
              mode={display}
            />
          )}
        </div>
        <p className="mt-2 text-2xs leading-relaxed text-muted-foreground">
          Bar length shows the total lines changed. Green is added; red is removed.
        </p>
      </div>
    </div>
  )
}
