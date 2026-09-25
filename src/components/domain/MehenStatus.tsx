import { ShieldAlert } from 'lucide-react'
import mehenMark from '@/assets/mehen-mark.png'
import { useMehenStatus, openInMehen } from '@/hooks/useMehen'
import type { MehenProblem } from '@/lib/bindings'
import { checkedWhen, isMehenStale, unsafePackages } from '@/lib/mehen'
import { cn } from '@/lib/utils'
import { useActiveRepo } from '@/stores/workspaceStore'
import { Button } from '@/components/ui/button'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'

const isSerious = (severity: string | null) => severity === 'CRITICAL' || severity === 'HIGH'

function severityLabel(severity: string | null) {
  switch (severity) {
    case 'CRITICAL':
      return 'Critical'
    case 'HIGH':
      return 'High'
    case 'MODERATE':
      return 'Medium'
    case 'LOW':
      return 'Low'
    default:
      return 'Unrated'
  }
}

function ProblemRow({ problem }: { problem: MehenProblem }) {
  return (
    <li className="flex flex-col gap-0.5 border-t border-border py-2 first:border-t-0 first:pt-0">
      <div className="flex items-baseline gap-2">
        <span
          className={cn(
            'w-14 flex-none text-2xs font-semibold uppercase',
            isSerious(problem.severity) ? 'text-[var(--gw-red)]' : 'text-[var(--gw-amber)]',
          )}
        >
          {severityLabel(problem.severity)}
        </span>
        <span className="min-w-0 truncate font-mono text-xs text-foreground">
          {problem.name}
          {problem.version ? ` ${problem.version}` : ''}
        </span>
        <span className="ml-auto flex-none text-2xs text-sub">fixed in {problem.fixed_in}</span>
      </div>
      <p className="line-clamp-2 pl-16 text-2xs text-sub">{problem.summary}</p>
    </li>
  )
}

/**
 * Dependency safety for the open repository, as Mehen last saw it.
 *
 * Silent unless Mehen found a package with a known security problem that has
 * a fix: a problem nobody can fix yet is not something to act on. Packages
 * with newer versions are too common to be worth space here, so they only
 * appear inside the popover. Silent too when Mehen has never checked the repo.
 */
export function MehenSegment() {
  const repo = useActiveRepo()
  const { status, canOpen } = useMehenStatus(repo?.path ?? null)

  if (!repo || !status || status.fixable === 0) return null

  const stale = isMehenStale(status.checked_at)
  const serious = status.problems.some((p) => isSerious(p.severity))
  const more = status.fixable - status.problems.length
  const label = `${status.fixable} unsafe ${status.fixable === 1 ? 'package' : 'packages'}`

  return (
    <Popover>
      <Tooltip>
        <TooltipTrigger asChild>
          <PopoverTrigger asChild>
            <button
              type="button"
              className={cn(
                'titlebar-no-drag flex items-center gap-1 rounded px-1 transition-colors hover:bg-panel3 active:bg-panel',
                stale ? 'text-muted-foreground' : serious ? 'text-[var(--gw-red)]' : 'text-[var(--gw-amber)]',
              )}
            >
              <ShieldAlert className="size-3" />
              <span>{label}</span>
            </button>
          </PopoverTrigger>
        </TooltipTrigger>
        <TooltipContent side="top">
          Mehen {checkedWhen(status.checked_at)}: {unsafePackages(status.fixable)}, and each has a fix. Click for details.
        </TooltipContent>
      </Tooltip>
      <PopoverContent align="start" side="top" className="w-96">
        <div className="flex flex-col gap-3 text-text">
          <div className="flex items-start gap-2.5">
            <img src={mehenMark} alt="" className="size-7 flex-none" draggable={false} />
            <div className="min-w-0">
              <p className="text-sm font-medium">{unsafePackages(status.fixable)}</p>
              <p className="text-xs text-sub">
                {stale
                  ? `Each has a fixed version to move to. Mehen ${checkedWhen(status.checked_at)}, so this may be out of date.`
                  : `Each has a fixed version to move to. From Mehen, ${checkedWhen(status.checked_at)}.`}
              </p>
            </div>
          </div>
          <ul className="m-0 flex list-none flex-col p-0">
            {status.problems.map((p) => (
              <ProblemRow key={`${p.ecosystem}:${p.name}`} problem={p} />
            ))}
          </ul>
          {(more > 0 || status.outdated > 0) && (
            <p className="text-xs text-sub">
              {more > 0 ? `${more} more in Mehen. ` : ''}
              {status.outdated > 0
                ? `${status.outdated} ${status.outdated === 1 ? 'package has' : 'packages have'} a newer version.`
                : ''}
            </p>
          )}
          {canOpen ? (
            <Button size="sm" className="self-start" onClick={() => void openInMehen(repo.id)}>
              <img src={mehenMark} alt="" className="size-4" draggable={false} />
              Fix these in Mehen
            </Button>
          ) : (
            <p className="text-xs text-sub">Open Mehen to update these packages.</p>
          )}
        </div>
      </PopoverContent>
    </Popover>
  )
}
