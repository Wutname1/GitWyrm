import { ChevronDown, ChevronRight } from 'lucide-react'
import mehenMark from '@/assets/mehen-mark.png'
import { MehenPackageList } from '@/components/domain/MehenStatus'
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { TooltipButton } from '@/components/ui/tooltip'
import { openInMehen, useMehenFlags } from '@/hooks/useMehen'
import { checkedWhen, MEHEN_TAB_LEVELS, mehenBadgeLabel, parseMehenTabLevel } from '@/lib/mehen'
import { cn } from '@/lib/utils'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo, useWorkspaceStore } from '@/stores/workspaceStore'

/**
 * The packages Mehen flags in this repository, at the level chosen in
 * Settings > Integrations or the menu in the section -- the same list the tab
 * badge counts. Absent when Mehen does not check this repository, the level is
 * set to nothing, or it lists nothing and the user chose to hide it then.
 */
export function MehenSection() {
  const repo = useActiveRepo()
  const { status, canOpen, level, badge, flagged } = useMehenFlags(repo?.path ?? null)
  const open = useUiStore((s) => s.sectionOpen.mehen)
  const toggleSection = useUiStore((s) => s.toggleSection)
  const hideWhenEmpty = useWorkspaceStore((s) => s.mehenHideWhenEmpty)

  if (!repo || !status || level === 'off') return null
  if (hideWhenEmpty && flagged.length === 0 && badge.count === 0) return null

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
          <MehenLevelMenu />
          {flagged.length > 0 ? (
            <MehenPackageList flagged={flagged} compact />
          ) : badge.count > 0 ? (
            // Counts with no list: written by a Mehen from before the list
            // existed. Its next check fills it in; Mehen has the list now.
            <p className="text-2xs text-sub">{mehenBadgeLabel(badge)}. Open Mehen to see which ones.</p>
          ) : (
            <p className="text-2xs text-sub">Nothing to update at this level.</p>
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

/**
 * Changes what the section lists right where it is shown. The same setting as
 * Settings > Integrations, so the tab badge and status bar follow it too.
 * "Nothing" stays in Settings: picking it here would remove the section the
 * menu lives in.
 */
function MehenLevelMenu() {
  const level = useWorkspaceStore((s) => s.mehenTabLevel)
  const setLevel = useWorkspaceStore((s) => s.setMehenTabLevel)
  const hideWhenEmpty = useWorkspaceStore((s) => s.mehenHideWhenEmpty)
  const setHideWhenEmpty = useWorkspaceStore((s) => s.setMehenHideWhenEmpty)
  const label = MEHEN_TAB_LEVELS.find((l) => l.id === level)?.label ?? ''

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <button
          type="button"
          className="-ml-1.5 flex min-w-0 items-center gap-1 self-start rounded px-1.5 py-0.5 text-2xs text-sub hover:bg-panel3 hover:text-foreground data-[state=open]:bg-panel3 data-[state=open]:text-foreground"
          title={`Showing: ${label}`}
        >
          <span className="flex-none">Showing:</span>
          <span className="min-w-0 truncate font-semibold text-foreground">{label}</span>
          <ChevronDown size={11} strokeWidth={2.4} className="flex-none" aria-hidden />
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="w-64">
        <DropdownMenuLabel className="text-2xs font-normal text-muted-foreground">
          Also sets the tab badge and status bar
        </DropdownMenuLabel>
        <DropdownMenuRadioGroup value={level} onValueChange={(v) => setLevel(parseMehenTabLevel(v))}>
          {MEHEN_TAB_LEVELS.filter((l) => l.id !== 'off').map((l) => (
            <DropdownMenuRadioItem key={l.id} value={l.id} className="text-xs">
              {l.label}
            </DropdownMenuRadioItem>
          ))}
        </DropdownMenuRadioGroup>
        <DropdownMenuSeparator />
        <DropdownMenuCheckboxItem className="text-xs" checked={hideWhenEmpty} onCheckedChange={(v) => setHideWhenEmpty(v === true)}>
          Hide this section when it's empty
        </DropdownMenuCheckboxItem>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
