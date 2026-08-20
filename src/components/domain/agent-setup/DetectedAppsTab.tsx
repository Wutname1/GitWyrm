import { CheckCircle2, CircleDashed } from 'lucide-react'
import type { ClientDetection } from '@/lib/bindings'
import { clientLabel } from '@/lib/agentConfig'

/**
 * "Detected apps" tab: which agent clients GitWyrm found configuration for
 * on this machine, and whether GitWyrm can write to them yet (task 4.6:
 * read-only clients stay visible, never hidden for lacking a writer).
 */
export function DetectedAppsTab({ detections }: { detections: ClientDetection[] }) {
  if (detections.length === 0) {
    return <p className="py-6 text-center text-2xs text-muted-foreground">Scanning for agent apps…</p>
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
            <span className="text-[10.5px] text-muted-foreground">
              {d.present ? 'Configuration found' : 'Not found on this machine'}
            </span>
          </div>
          <span className="text-[10.5px] text-muted-foreground">
            {d.writeSupported ? 'Read + write' : 'Read only'}
          </span>
        </li>
      ))}
    </ul>
  )
}
