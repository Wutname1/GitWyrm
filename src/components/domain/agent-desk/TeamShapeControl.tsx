import { ChevronUp, GitFork, User } from 'lucide-react'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import type { ComposerTeam } from '@/lib/agentDeskComposer'
import { cn } from '@/lib/utils'

/**
 * "Who works on this chat?" popover: Solo agent vs. Lead + helpers.
 *
 * Independent of `OperatingModeControl` (tasks.md 6.2) -- this only decides
 * how many agents can exist for the session; Ask/Plan/Auto separately
 * decides what they are allowed to do once running. Structure mirrors the
 * mockup's `.ag-team-trigger` / `.ag-team-popover` (a trigger button that
 * opens a small card with two option rows), colors from app tokens.
 */
export function TeamShapeControl({
  team,
  onChange,
  open,
  onOpenChange,
}: {
  team: ComposerTeam
  onChange: (team: ComposerTeam) => void
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  return (
    <Popover open={open} onOpenChange={onOpenChange}>
      <PopoverTrigger asChild>
        <button
          type="button"
          className="flex flex-none items-center gap-1 rounded px-1.5 py-0.5 text-2xs font-semibold text-sub hover:bg-panel3 hover:text-foreground"
        >
          <GitFork size={12} />
          {team === 'solo' ? 'Solo agent' : 'Lead + helpers'}
          <ChevronUp size={11} />
        </button>
      </PopoverTrigger>
      <PopoverContent side="top" align="start" className="w-72 p-2">
        <div className="mb-2 flex items-center gap-2">
          <GitFork size={13} className="text-accent-text" />
          <strong className="text-xs font-semibold">Who works on this chat?</strong>
          <span className="ml-auto text-[10px] text-muted-foreground">Change at any time</span>
        </div>
        <div className="flex flex-col gap-1.5">
          <button
            type="button"
            onClick={() => {
              onChange('solo')
              onOpenChange(false)
            }}
            className={cn(
              'flex items-start gap-2 rounded-md border border-border px-2 py-1.5 text-left',
              team === 'solo' ? 'border-primary/50 bg-soft' : 'hover:bg-panel3'
            )}
          >
            <User size={14} className="mt-0.5 flex-none text-muted-foreground" />
            <span className="min-w-0 flex-1">
              <strong className="block text-2xs font-semibold text-foreground">Solo agent</strong>
              <span className="block text-[10.5px] leading-snug text-muted-foreground">
                One agent owns the chat. No graph is created.
              </span>
            </span>
            <span className="flex-none font-mono text-[9px] text-muted-foreground">1 agent</span>
          </button>
          <button
            type="button"
            onClick={() => {
              onChange('helpers')
              onOpenChange(false)
            }}
            className={cn(
              'flex items-start gap-2 rounded-md border border-border px-2 py-1.5 text-left',
              team === 'helpers' ? 'border-primary/50 bg-soft' : 'hover:bg-panel3'
            )}
          >
            <GitFork size={14} className="mt-0.5 flex-none text-muted-foreground" />
            <span className="min-w-0 flex-1">
              <strong className="block text-2xs font-semibold text-foreground">Lead + helpers</strong>
              <span className="block text-[10.5px] leading-snug text-muted-foreground">
                The lead splits safe work between helpers. Plan lets you approve the split first; Auto starts it when useful.
              </span>
            </span>
            <span className="flex-none font-mono text-[9px] text-muted-foreground">up to 3</span>
          </button>
        </div>
      </PopoverContent>
    </Popover>
  )
}
