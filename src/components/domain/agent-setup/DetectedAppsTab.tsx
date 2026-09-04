import { AlertTriangle, CheckCircle2, CircleDashed } from 'lucide-react'
import type { ClientDetection } from '@/lib/bindings'
import { clientLabel } from '@/lib/agentConfig'

/**
 * "Detected apps" tab: which agent clients GitWyrm found configuration for
 * on this machine, and whether GitWyrm can write to them yet (task 4.6:
 * read-only clients stay visible, never hidden for lacking a writer).
 */
export function DetectedAppsTab({
  detections,
  isLoading = false,
  isError = false,
  onRetry,
}: {
  detections: ClientDetection[]
  /**
   * Whether the scan is running or failed.
   *
   * An empty list used to render "Scanning for agent apps…" forever, which
   * covered three different situations: still looking, could not look, and
   * genuinely none installed. Only the first is what that sentence says.
   */
  isLoading?: boolean
  isError?: boolean
  onRetry?: () => void
}) {
  if (isLoading && detections.length === 0) {
    return <p className="py-6 text-center text-2xs text-muted-foreground">Scanning for agent apps…</p>
  }
  if (isError && detections.length === 0) {
    return (
      <div className="rounded-md border border-destructive/40 bg-destructive/10 p-3">
        <p className="flex items-center gap-1.5 text-2xs font-semibold text-destructive">
          <AlertTriangle size={13} aria-hidden />
          GitWyrm could not look for other AI apps on this computer.
        </p>
        <p className="mt-1 text-2xs text-muted-foreground">This is not the same as having none installed.</p>
        {onRetry && (
          <button
            type="button"
            onClick={onRetry}
            disabled={isLoading}
            className="mt-2 rounded border border-border px-2 py-1 text-2xs font-semibold hover:bg-panel3"
          >
            {isLoading ? 'Checking…' : 'Try again'}
          </button>
        )}
      </div>
    )
  }
  if (detections.length === 0) {
    return (
      <p className="py-6 text-center text-2xs text-muted-foreground">
        No other AI apps were found on this computer.
      </p>
    )
  }

  return (
    <ul className="flex flex-col gap-2">
      {detections.map((d) => (
        <li
          key={d.client}
          className="flex items-center gap-2.5 rounded-md border border-border px-3 py-2.5"
        >
          {d.present ? (
            <CheckCircle2 size={15} className="flex-none text-[var(--gw-green)]" aria-hidden />
          ) : (
            <CircleDashed size={15} className="flex-none text-muted-foreground" aria-hidden />
          )}
          <div className="flex flex-1 flex-col gap-0.5">
            <span className="text-xs font-medium text-foreground">{clientLabel(d.client)}</span>
            <span className="text-2xs text-muted-foreground">
              {d.present ? 'Configuration found' : 'Not found on this machine'}
            </span>
          </div>
          <span className="text-2xs text-muted-foreground">
            {d.writeSupported ? 'Read + write' : 'Read only'}
          </span>
        </li>
      ))}
    </ul>
  )
}
