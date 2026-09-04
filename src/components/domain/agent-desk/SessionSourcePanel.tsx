import { MessageSquareText } from 'lucide-react'
import type { AgentSession } from '@/lib/bindings'
import { sourceKindLabel } from '@/lib/agentSessionGrouping'
import { openSpecProgressLine, openSpecStatusLine } from '@/lib/openSpecSessionStatus'
import { useOpenSpecSessionContext, useOpenSpecSessionStatus } from '@/hooks/useOpenspecSessionSource'
import { describeSnapshotFreshness } from '@/lib/agentDeskSources'
import { describeSource, SOURCE_KIND_ICON } from './SessionSourceBanner'

/**
 * The Source detail's content, extracted from `SessionSourceBanner` so it
 * can render inside the per-pane popover or the docked panel host (tasks.md
 * 6.6, 7.1) -- both places the *large* inline banner is not shown, but the
 * information it carries must stay one click away.
 *
 * Deliberately session-scoped (`session: AgentSession`, not a session ID):
 * every panel behind the dock host reads its content this same way so the
 * host can swap `session` and every panel refreshes without owning its own
 * fetch (tasks.md 7.2/7.9).
 */
export function SessionSourcePanel({ session, onOpenSource }: { session: AgentSession; onOpenSource?: () => void }) {
  const { header } = session
  const { source } = header
  const { kicker, title, meta } = describeSource(source)
  const Icon = SOURCE_KIND_ICON[source.kind] ?? MessageSquareText
  const liveUnavailable = source.kind !== 'manual' && source.snapshot.liveUnavailable
  // How old the saved copy is. Without this the panel shows a summary that
  // reads as current no matter how long ago it was captured.
  const freshness =
    source.kind === 'manual' ? null : describeSnapshotFreshness(source.snapshot.capturedAt, liveUnavailable)
  const missingSource = header.state === 'missingSource'
  const isOpenSpecSource = source.kind === 'openSpecChange' || source.kind === 'openSpecTask'

  // Only asked for an OpenSpec source, and only once the generic snapshot
  // already thinks something changed -- this is the honest archived/moved/
  // deleted breakdown behind that flag, not a second independent check that
  // could disagree with it (tasks.md 4.5/section 7).
  const openSpecStatus = useOpenSpecSessionStatus(session.header.sessionId, isOpenSpecSource && liveUnavailable)
  const statusLine = isOpenSpecSource ? openSpecStatusLine(openSpecStatus.data) : null

  // How far along the change is. Asked only for an OpenSpec source, and only
  // while the source is still there -- once it is gone the honest answer is
  // the status line above, not a progress count from a stale copy.
  const openSpecContext = useOpenSpecSessionContext(session.header.sessionId, isOpenSpecSource && !liveUnavailable)
  const progressLine = isOpenSpecSource ? openSpecProgressLine(openSpecContext.data) : null

  return (
    <div className="flex flex-col gap-2 p-2">
      <div className="flex items-start gap-2">
        <Icon size={16} className="mt-0.5 flex-none text-muted-foreground" aria-label={sourceKindLabel(source.kind)} />
        <div className="min-w-0 flex-1">
          <p className="text-2xs font-bold uppercase tracking-wide text-accent-text">{kicker}</p>
          <p className="truncate text-xs font-semibold text-foreground">{title}</p>
          <p className="truncate font-mono text-2xs text-sub">{meta}</p>
        </div>
      </div>

      {missingSource ? (
        <p className="text-2xs font-semibold text-[var(--gw-red)]">Could not load this source.</p>
      ) : statusLine ? (
        <p
          className={
            statusLine.tone === 'red'
              ? 'text-2xs font-semibold text-[var(--gw-red)]'
              : statusLine.tone === 'amber'
                ? 'text-2xs font-semibold text-[var(--gw-amber)]'
                : 'text-2xs font-semibold text-muted-foreground'
          }
        >
          {statusLine.text}
        </p>
      ) : liveUnavailable ? (
        <p className="text-2xs font-semibold text-[var(--gw-amber)]">No longer available.</p>
      ) : null}

      {progressLine && <p className="text-2xs font-semibold text-foreground">{progressLine}</p>}

      {freshness && <p className="text-2xs leading-relaxed text-muted-foreground">{freshness}</p>}

      <button
        type="button"
        onClick={onOpenSource}
        disabled={!onOpenSource}
        className="self-start rounded px-1.5 py-1 text-2xs font-semibold text-accent-text hover:bg-soft disabled:cursor-not-allowed disabled:text-muted-foreground disabled:hover:bg-transparent"
      >
        Open the source
      </button>
    </div>
  )
}
