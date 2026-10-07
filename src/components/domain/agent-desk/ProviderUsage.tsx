import { useQuery } from '@tanstack/react-query'
import { Gauge, RefreshCw } from 'lucide-react'
import { commands, type ProviderUsage, type UsageWindow } from '@/lib/bindings'
import { unwrap } from '@/lib/queryKeys'
import { cn } from '@/lib/utils'
import { formatCompactAge } from '@/lib/agentSessionGrouping'
import { formatResetsIn, formatUsedPercent, fullestWindow, usageTone, type UsageTone } from '@/lib/providerUsage'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'

export const PROVIDER_USAGE_KEY = ['providerUsage'] as const

/**
 * Plan limits for every AI tool set up on this machine.
 *
 * The backend caches each source and asks it no more often than it allows
 * (Claude every 5 minutes, Copilot every 10), so polling here is cheap: most
 * polls are answered from that cache.
 */
export function useProviderUsage() {
  return useQuery({
    queryKey: PROVIDER_USAGE_KEY,
    queryFn: async () => unwrap(await commands.agentProviderUsage(false)),
    staleTime: 60_000,
    refetchInterval: 5 * 60_000,
  })
}

/** The usage entry for one tool id ("claude", "codex", "copilot"). */
export function useUsageFor(provider: string | null | undefined): ProviderUsage | null {
  const { data } = useProviderUsage()
  if (!data || !provider) return null
  return data.find((u) => u.provider === provider) ?? null
}

const TONE_STROKE: Record<UsageTone, string> = {
  ok: 'var(--gw-accent)',
  warn: 'var(--gw-amber)',
  high: 'var(--destructive)',
}

/** A small ring that fills as a limit is used up. */
export function UsageRing({ percent, size = 14 }: { percent: number; size?: number }) {
  const clamped = Math.min(100, Math.max(0, percent))
  const stroke = 2
  const r = (size - stroke) / 2
  const c = 2 * Math.PI * r
  return (
    <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`} className="flex-none -rotate-90" aria-hidden>
      <circle cx={size / 2} cy={size / 2} r={r} fill="none" stroke="var(--gw-border)" strokeWidth={stroke} />
      <circle
        cx={size / 2}
        cy={size / 2}
        r={r}
        fill="none"
        stroke={TONE_STROKE[usageTone(clamped)]}
        strokeWidth={stroke}
        strokeLinecap="round"
        strokeDasharray={`${(clamped / 100) * c} ${c}`}
      />
    </svg>
  )
}

const TONE_BAR: Record<UsageTone, string> = {
  ok: 'bg-primary',
  warn: 'bg-[var(--gw-amber)]',
  high: 'bg-destructive',
}

function WindowBar({ window }: { window: UsageWindow }) {
  const resets = formatResetsIn(window.resetsAt)
  return (
    <div className="flex flex-col gap-1">
      <div className="flex items-baseline justify-between gap-2 text-2xs">
        <span className="font-medium text-foreground">{window.label}</span>
        <span className="text-muted-foreground">{formatUsedPercent(window.usedPercent)}</span>
      </div>
      <div
        className="h-1.5 overflow-hidden rounded-full bg-panel3"
        role="progressbar"
        aria-label={window.label}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={Math.round(window.usedPercent)}
      >
        <div
          className={cn('h-full rounded-full', TONE_BAR[usageTone(window.usedPercent)])}
          style={{ width: `${Math.min(100, Math.max(0, window.usedPercent))}%` }}
        />
      </div>
      {(resets || window.detail) && (
        <p className="text-2xs text-muted-foreground">{[window.detail, resets].filter(Boolean).join(' · ')}</p>
      )}
    </div>
  )
}

/** One tool's limits: its plan, one bar per window, or why there are none. */
export function ProviderUsageBlock({ usage }: { usage: ProviderUsage }) {
  return (
    <section className="flex flex-col gap-2">
      <header className="flex items-baseline gap-1.5">
        <h3 className="text-xs font-semibold text-foreground">{usage.displayName}</h3>
        {usage.plan && <span className="text-2xs text-muted-foreground">{usage.plan}</span>}
        {usage.fetchedAt && (
          <span className="ml-auto text-2xs text-muted-foreground">
            {formatCompactAge(usage.fetchedAt) === 'now' ? 'just now' : `${formatCompactAge(usage.fetchedAt)} ago`}
          </span>
        )}
      </header>
      {usage.windows.length > 0 ? (
        usage.windows.map((w) => <WindowBar key={`${w.kind}-${w.label}`} window={w} />)
      ) : (
        <p className="text-2xs leading-relaxed text-muted-foreground">
          {usage.unavailableReason ?? 'This tool did not report any limits.'}
        </p>
      )}
    </section>
  )
}

/** Every tool's limits, with a way to ask again. */
export function ProviderUsageList({ className }: { className?: string }) {
  const query = useProviderUsage()
  const list = query.data ?? []
  return (
    <div className={cn('flex flex-col gap-3', className)}>
      {query.isLoading ? (
        <p className="text-2xs text-muted-foreground">Checking your plan limits…</p>
      ) : list.length === 0 ? (
        <p className="text-2xs leading-relaxed text-muted-foreground">
          {query.isError
            ? 'Could not check your plan limits just now.'
            : 'No AI tools with plan limits were found on this computer.'}
        </p>
      ) : (
        list.map((u, i) => (
          <div key={u.provider} className={cn(i > 0 && 'border-t border-border pt-3')}>
            <ProviderUsageBlock usage={u} />
          </div>
        ))
      )}
      <button
        type="button"
        onClick={() => void commands.agentProviderUsage(true).then(() => query.refetch())}
        disabled={query.isFetching}
        className="flex items-center gap-1 self-start rounded px-1.5 py-0.5 text-2xs font-semibold text-sub hover:bg-panel3 hover:text-foreground disabled:opacity-50"
      >
        <RefreshCw size={11} className={cn(query.isFetching && 'animate-spin motion-reduce:animate-none')} aria-hidden />
        {query.isFetching ? 'Checking…' : 'Check again'}
      </button>
    </div>
  )
}

/**
 * The title bar's way in to every tool's limits: a ring showing whichever
 * limit is closest to running out, opening the full list.
 */
export function PlanLimitsButton() {
  const { data } = useProviderUsage()
  const worst = (data ?? [])
    .map((u) => ({ usage: u, window: fullestWindow(u) }))
    .filter((x): x is { usage: ProviderUsage; window: UsageWindow } => x.window != null)
    .sort((a, b) => b.window.usedPercent - a.window.usedPercent)[0]
  const label = worst
    ? `Plan limits: ${worst.usage.displayName} ${worst.window.label.toLowerCase()} ${formatUsedPercent(worst.window.usedPercent)}`
    : 'Plan limits'
  return (
    <Popover>
      <PopoverTrigger asChild>
        <button
          type="button"
          aria-label={label}
          title={label}
          className="flex h-[26px] flex-none items-center justify-center gap-1 rounded border border-transparent px-1 text-sub hover:border-border hover:bg-panel3 hover:text-foreground"
        >
          {worst ? <UsageRing percent={worst.window.usedPercent} /> : <Gauge size={14} aria-hidden />}
        </button>
      </PopoverTrigger>
      <PopoverContent align="end" className="w-80">
        <h2 className="mb-2 text-xs font-semibold text-foreground">Plan limits</h2>
        <ProviderUsageList />
      </PopoverContent>
    </Popover>
  )
}
