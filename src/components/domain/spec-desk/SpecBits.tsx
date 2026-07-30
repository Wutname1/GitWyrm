import { cn } from '@/lib/utils'
import type { ChangeStatus, DeltaKind, SpecChange } from '@/lib/bindings'
import { STATUS_LABEL, statusTone } from '@/lib/specDisplay'

/**
 * Small shared pieces used by both the main-window spec card and the Desk.
 * Extracted so a status colour or ring size can only be changed in one place --
 * two windows showing the same change must not describe it differently.
 */

const TONE_CLASS: Record<ReturnType<typeof statusTone>, string> = {
  muted: 'border-muted-foreground/40 text-muted-foreground',
  info: 'border-[var(--gw-blue)]/40 text-[var(--gw-blue)] bg-[var(--gw-blue)]/8',
  warn: 'border-[var(--gw-amber)]/40 text-[var(--gw-amber)] bg-[var(--gw-amber)]/8',
  good: 'border-primary/40 text-accent-text bg-soft',
}

export function StatusPill({
  status,
  className,
}: {
  status: ChangeStatus
  className?: string
}) {
  return (
    <span
      className={cn(
        'flex-none whitespace-nowrap rounded-full border px-1.5 py-px text-2xs font-semibold',
        TONE_CLASS[statusTone(status)],
        className
      )}
    >
      {STATUS_LABEL[status]}
    </span>
  )
}

/** A ring showing percent complete. `size` is the outer diameter in pixels. */
export function ProgressRing({ percent, size = 44 }: { percent: number; size?: number }) {
  // The inner cutout has to scale with the ring or a large ring looks solid.
  const inset = Math.max(3, Math.round(size * 0.1))
  return (
    <div
      className="relative grid flex-none place-items-center rounded-full"
      style={{
        width: size,
        height: size,
        background: `conic-gradient(var(--gw-accent) ${percent}%, var(--gw-panel3) 0)`,
      }}
      role="img"
      aria-label={`${percent} percent done`}
    >
      <span className="absolute rounded-full bg-panel" style={{ inset }} />
      <span className="relative font-mono text-2xs font-semibold text-foreground">{percent}%</span>
    </div>
  )
}

const DELTA_CLASS: Record<DeltaKind, string> = {
  ADDED: 'bg-soft text-accent-text',
  MODIFIED: 'bg-[var(--gw-amber)]/15 text-[var(--gw-amber)]',
  REMOVED: 'bg-[var(--gw-red)]/15 text-removed',
  RENAMED: 'bg-[var(--gw-blue)]/15 text-[var(--gw-blue)]',
}

/** ADDED / MODIFIED / REMOVED badge on a spec delta. */
export function DeltaBadge({ kind }: { kind: DeltaKind }) {
  return (
    <span
      className={cn(
        'flex-none rounded px-1.5 py-px font-mono text-2xs font-bold tracking-wide',
        DELTA_CLASS[kind]
      )}
    >
      {kind}
    </span>
  )
}

/** Thin progress bar with the count beside it. */
export function ProgressBar({ change }: { change: SpecChange }) {
  return (
    <span className="block h-[3px] overflow-hidden rounded-full bg-panel3">
      <span
        className="block h-full rounded-full bg-primary transition-[width] duration-300"
        style={{ width: `${change.progress.percent}%` }}
      />
    </span>
  )
}
