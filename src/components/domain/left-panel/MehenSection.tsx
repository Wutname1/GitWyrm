import { ChevronRight } from 'lucide-react'
import mehenMark from '@/assets/mehen-mark.png'
import { MehenPackageList } from '@/components/domain/MehenStatus'
import { TooltipButton } from '@/components/ui/tooltip'
import { openInMehen, useMehenFlags } from '@/hooks/useMehen'
import { checkedWhen, mehenBadgeLabel } from '@/lib/mehen'
import { cn } from '@/lib/utils'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo } from '@/stores/workspaceStore'

/**
 * The packages Mehen flags in this repository, at the level chosen in
 * Settings > Integrations -- the same list the tab badge counts. Absent when
 * Mehen does not check this repository or the level is set to nothing.
 */
export function MehenSection() {
  const repo = useActiveRepo()
  const { status, canOpen, level, badge, flagged } = useMehenFlags(repo?.path ?? null)
  const open = useUiStore((s) => s.sectionOpen.mehen)
  const toggleSection = useUiStore((s) => s.toggleSection)

  if (!repo || !status || level === 'off') return null

  const fix = badge.security > 0
  const openMehen = () => void openInMehen(repo.id, fix)

  return (
    <div className="group/section">
      <div
        onClick={() => toggleSection('mehen')}
        className="flex cursor-pointer select-none items-center gap-1.5 py-1.5 pl-2.5 pr-3 hover:bg-panel2"
      >
        <ChevronRight
          size={12}
          strokeWidth={2.4}
          className={cn('flex-none text-muted-foreground transition-transform duration-100', open && 'rotate-90')}
        />
        <span className="text-2xs font-bold tracking-[.09em] text-sub">MEHEN</span>
        {canOpen && (
          <TooltipButton
            onClick={(e) => {
              e.stopPropagation()
              openMehen()
            }}
            tooltip={fix ? 'Fix these in Mehen' : 'Open in Mehen'}
            className="ml-auto flex flex-none items-center rounded-sm p-0.5 opacity-0 hover:bg-panel3 group-hover/section:opacity-100 focus-visible:opacity-100"
          >
            <img src={mehenMark} alt="" className="size-3.5" draggable={false} />
          </TooltipButton>
        )}
        <span
          className={cn(
            'flex-none font-mono text-2xs',
            !canOpen && 'ml-auto',
            badge.count === 0 ? 'text-muted-foreground' : fix ? 'text-[var(--gw-red)]' : 'text-[var(--gw-amber)]',
          )}
        >
          {badge.count}
        </span>
      </div>

      {open && (
        <div className="flex flex-col gap-2 pb-2 pl-7 pr-3">
          {flagged.length > 0 ? (
            <MehenPackageList flagged={flagged} compact />
          ) : badge.count > 0 ? (
            // Counts with no list: written by a Mehen from before the list
            // existed. Its next check fills it in; Mehen has the list now.
            <p className="text-2xs text-sub">{mehenBadgeLabel(badge)}. Open Mehen to see which ones.</p>
          ) : (
            <p className="text-2xs text-sub">Nothing to update at the level you chose.</p>
          )}
          <div className="flex items-center gap-2 text-2xs text-sub">
            <span className="min-w-0 flex-1 truncate" title={badge.count > 0 ? mehenBadgeLabel(badge) : undefined}>
              {checkedWhen(status.checked_at)}
            </span>
            {canOpen && (
              <button
                type="button"
                onClick={openMehen}
                className="flex flex-none items-center gap-1 rounded px-1.5 py-0.5 text-foreground hover:bg-panel3 active:bg-panel"
              >
                <img src={mehenMark} alt="" className="size-3.5" draggable={false} />
                {fix ? 'Fix in Mehen' : 'Open in Mehen'}
              </button>
            )}
          </div>
        </div>
      )}
    </div>
  )
}
