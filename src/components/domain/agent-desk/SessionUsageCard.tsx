import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { ChevronDown, TimerReset } from 'lucide-react'
import { commands } from '@/lib/bindings'
import { keys, unwrap } from '@/lib/queryKeys'
import { buildUsageRows } from '@/lib/agentDeskUsage'
import { cn } from '@/lib/utils'

/**
 * Collapsible session usage card (tasks.md 7.3/7.4).
 *
 * Reads `agentSessionUsage`, whose `SessionUsage` fields are all optional
 * (architecture.md section 12: "Every field is optional and carries
 * `source: measured | provider_reported | estimated`"). `buildUsageRows`
 * (in `src/lib/agentDeskUsage.ts`) is the single place that decides which
 * rows exist at all -- an absent field produces no row, never a "0" row --
 * so this component only has to render whatever comes back, plus mark any
 * `estimated` row as an estimate in its accessible label (never shown as if
 * it were measured).
 *
 * Collapsed state persists per session in component state for the life of
 * the mount; `defaultCollapsed` starts open the first time, matching the
 * mockup's default (`.ag-usage-card` without `.is-collapsed`).
 */
export function SessionUsageCard({ sessionId }: { sessionId: string }) {
  const [collapsed, setCollapsed] = useState(false)
  const query = useQuery({
    queryKey: keys.agentSessionUsage(sessionId),
    queryFn: async () => unwrap(await commands.agentSessionUsage(sessionId)),
    // Usage numbers are not live-pushed; a light poll keeps "Current
    // session" roughly fresh without needing a dedicated event.
    refetchInterval: 30_000,
  })

  const usage = query.data?.kind === 'available' ? query.data.usage : null
  const rows = usage ? buildUsageRows(usage) : []

  return (
    <section className="rounded-md border border-border bg-panel2">
      <button
        type="button"
        onClick={() => setCollapsed((c) => !c)}
        aria-expanded={!collapsed}
        className="flex w-full items-center gap-1.5 px-2 py-1.5 text-left"
      >
        <TimerReset size={13} className="flex-none text-muted-foreground" aria-hidden />
        <strong className="text-2xs font-semibold text-foreground">Usage</strong>
        <span className="flex-1 truncate text-[10px] text-muted-foreground">session + overall</span>
        <ChevronDown size={13} className={cn('flex-none text-muted-foreground transition-transform', collapsed && '-rotate-90')} />
      </button>

      {!collapsed && (
        <div className="border-t border-border px-2 py-1.5">
          {query.isLoading ? (
            <p className="py-1 text-2xs text-muted-foreground">Loading usage…</p>
          ) : rows.length === 0 ? (
            // Usage honesty (tasks.md 7.4): nothing measured yet is stated
            // plainly, never rendered as a row of zeros.
            <p className="py-1 text-2xs leading-relaxed text-muted-foreground">
              No usage data yet for this chat.
            </p>
          ) : (
            <dl className="flex flex-col gap-1">
              {rows.map((row) => (
                <div key={row.key} className="flex items-baseline justify-between gap-2 text-2xs">
                  <dt className="text-muted-foreground">{row.label}</dt>
                  <dd className="min-w-0 truncate text-right font-semibold text-foreground">
                    {row.value}
                    {row.isEstimate && (
                      <span
                        className="ml-1 align-middle text-[9px] font-normal uppercase tracking-wide text-muted-foreground"
                        aria-label={`${row.label} is an estimate, not a measured value`}
                      >
                        est.
                      </span>
                    )}
                  </dd>
                </div>
              ))}
            </dl>
          )}
        </div>
      )}
    </section>
  )
}
