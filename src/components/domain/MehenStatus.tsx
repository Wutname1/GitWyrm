import { Package, ShieldAlert } from 'lucide-react'
import mehenMark from '@/assets/mehen-mark.png'
import { useMehenFlags, openInMehen } from '@/hooks/useMehen'
import type { MehenFlagged } from '@/lib/bindings'
import { MEHEN_LEVEL_LABEL, checkedWhen, isMehenStale, isSecurityLevel, mehenBadgeLabel, type MehenLevel } from '@/lib/mehen'
import { cn } from '@/lib/utils'
import { useActiveRepo, useWorkspaceStore } from '@/stores/workspaceStore'
import { Button } from '@/components/ui/button'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'

/** Serious security in red, the rest of security in amber, plain updates in blue. */
export function mehenLevelTone(level: string) {
  if (level === 'critical' || level === 'high') return 'text-[var(--gw-red)]'
  if (isSecurityLevel(level)) return 'text-[var(--gw-amber)]'
  return 'text-[var(--gw-blue)]'
}

/**
 * The packages Mehen flags, most urgent first: how urgent, the package, and
 * the version to move to. `compact` drops the advisory line to fit the sidebar;
 * it stays available on hover.
 */
export function MehenPackageList({ flagged, compact = false }: { flagged: MehenFlagged[]; compact?: boolean }) {
  return (
    <ul className="m-0 flex list-none flex-col p-0">
      {flagged.map((f) => {
        const move = f.target ? `${f.version ?? '?'} → ${f.target}` : (f.version ?? '')
        return (
          <li
            key={`${f.ecosystem}:${f.name}`}
            title={[`${f.name} ${move}`, f.summary].filter(Boolean).join('\n')}
            className={cn('flex flex-col gap-0.5', compact ? 'py-0.5' : 'border-t border-border py-1.5 first:border-t-0 first:pt-0')}
          >
            <div className="flex items-baseline gap-2">
              <span className={cn('w-12 flex-none text-2xs font-semibold', mehenLevelTone(f.level))}>
                {MEHEN_LEVEL_LABEL[f.level as MehenLevel] ?? f.level}
              </span>
              <span className="min-w-0 truncate font-mono text-xs text-foreground">{f.name}</span>
              <span className="ml-auto flex-none font-mono text-2xs text-sub">{compact ? (f.target ?? '') : move}</span>
            </div>
            {!compact && f.summary && <p className="line-clamp-2 pl-14 text-2xs text-sub">{f.summary}</p>}
          </li>
        )
      })}
    </ul>
  )
}

/** Short enough for the status bar. */
function segmentLabel({ count, security }: { count: number; security: number }) {
  if (security === count) return `${count} unsafe ${count === 1 ? 'package' : 'packages'}`
  const updates = `${count} package ${count === 1 ? 'update' : 'updates'}`
  return security > 0 ? `${updates} (${security} unsafe)` : updates
}

/**
 * What Mehen flags in the open repository, at the level chosen in settings --
 * the same count its tab shows. Silent when there is nothing at that level,
 * when Mehen has never checked the repository, or when turned off.
 */
export function MehenSegment() {
  const repo = useActiveRepo()
  const { status, canOpen, badge, flagged } = useMehenFlags(repo?.path ?? null)
  const enabled = useWorkspaceStore((s) => s.mehenShowStatus)

  if (!enabled || !repo || !status || badge.count === 0) return null

  const stale = isMehenStale(status.checked_at)
  const fix = badge.security > 0

  return (
    <Popover>
      <Tooltip>
        <TooltipTrigger asChild>
          <PopoverTrigger asChild>
            <button
              type="button"
              className={cn(
                'titlebar-no-drag flex items-center gap-1 rounded px-1 transition-colors hover:bg-panel3 active:bg-panel',
                stale ? 'text-muted-foreground' : fix ? 'text-[var(--gw-red)]' : 'text-[var(--gw-amber)]',
              )}
            >
              {fix ? <ShieldAlert className="size-3" /> : <Package className="size-3" />}
              <span>{segmentLabel(badge)}</span>
            </button>
          </PopoverTrigger>
        </TooltipTrigger>
        <TooltipContent side="top">
          Mehen {checkedWhen(status.checked_at)}: {mehenBadgeLabel(badge)}. Click for the list.
        </TooltipContent>
      </Tooltip>
      <PopoverContent align="start" side="top" className="w-96">
        <div className="flex flex-col gap-3 text-text">
          <div className="flex items-start gap-2.5">
            <img src={mehenMark} alt="" className="size-7 flex-none" draggable={false} />
            <div className="min-w-0">
              <p className="text-sm font-medium">{mehenBadgeLabel(badge)}</p>
              <p className="text-xs text-sub">
                {stale
                  ? `Mehen ${checkedWhen(status.checked_at)}, so this may be out of date.`
                  : `From Mehen, ${checkedWhen(status.checked_at)}. Security problems show only when they have a fix.`}
              </p>
            </div>
          </div>
          {flagged.length > 0 ? (
            <div className="max-h-72 overflow-y-auto">
              <MehenPackageList flagged={flagged} />
            </div>
          ) : (
            <p className="text-xs text-sub">Open Mehen to see which packages. Its next check lists them here.</p>
          )}
          {canOpen ? (
            <Button size="sm" className="self-start" onClick={() => void openInMehen(repo.id, fix)}>
              <img src={mehenMark} alt="" className="size-4" draggable={false} />
              {fix ? 'Fix these in Mehen' : 'Update these in Mehen'}
            </Button>
          ) : (
            <p className="text-xs text-sub">Open Mehen to update these packages.</p>
          )}
        </div>
      </PopoverContent>
    </Popover>
  )
}
