import {
  CircleAlert,
  CircleDot,
  Download,
  FileCheck2,
  FileDiff,
  GitBranch,
  GitCommitHorizontal,
  GitPullRequest,
  ListTree,
  MessageSquareText,
} from 'lucide-react'
import type { SessionSource, SessionState } from '@/lib/bindings'
import { describeSnapshotFreshness } from '@/lib/agentDeskSources'
import { adapterDisplayName } from '@/lib/agentImportDisplay'
import { sourceKindLabel } from '@/lib/agentSessionGrouping'
import { useSessionReadOnly } from '@/hooks/useAgentSessions'

// Keyed by the real union rather than `string`, so a new source kind without
// an icon is a compile error. As `Record<string, ...>` the `imported` kind was
// simply absent and fell back to the manual-chat glyph -- the same picture for
// a conversation brought in from elsewhere as for one started here.
export const SOURCE_KIND_ICON: Record<
  SessionSource['kind'],
  React.ComponentType<{ size?: number; className?: string }>
> = {
  manual: MessageSquareText,
  issue: CircleDot,
  pullRequest: GitPullRequest,
  openSpecChange: FileCheck2,
  openSpecTask: ListTree,
  commit: GitCommitHorizontal,
  diff: FileDiff,
  workingChanges: GitBranch,
  checkFailure: CircleAlert,
  imported: Download,
}

/**
 * Plain-language kicker/title/meta for the source banner, per source kind.
 * Exported so `SessionSourcePanel` (the popover/dock content behind the
 * per-pane Source button, tasks.md 6.6/7.1) describes the same source the
 * same way instead of duplicating this switch.
 */
export function describeSource(source: SessionSource): { kicker: string; title: string; meta: string } {
  switch (source.kind) {
    case 'manual':
      return { kicker: 'New chat', title: 'Started without a specific issue, PR, or task', meta: 'Manual chat' }
    case 'issue':
      return {
        kicker: `Started from Issue #${source.number}`,
        title: source.snapshot.title,
        meta: `${source.owner}/${source.repo}`,
      }
    case 'pullRequest':
      return {
        kicker: `Started from Pull request #${source.number}`,
        title: source.snapshot.title,
        meta: `${source.owner}/${source.repo} · ${source.head} → ${source.base}`,
      }
    case 'openSpecChange':
      return {
        kicker: 'Started from OpenSpec change',
        title: source.snapshot.title,
        meta: source.changeId,
      }
    case 'openSpecTask':
      return {
        kicker: 'Started from OpenSpec task',
        title: source.snapshot.title,
        meta: `${source.changeId} · task ${source.taskIndex + 1}`,
      }
    case 'commit':
      return {
        kicker: 'Started from a commit',
        title: source.snapshot.title,
        meta: source.oid.slice(0, 10),
      }
    case 'diff':
      return {
        kicker: 'Started from a diff',
        title: source.snapshot.title,
        meta: `${source.paths.length} file${source.paths.length === 1 ? '' : 's'}`,
      }
    case 'workingChanges':
      return {
        kicker: 'Started from working changes',
        title: source.snapshot.title,
        meta: `${source.paths.length} file${source.paths.length === 1 ? '' : 's'}`,
      }
    case 'imported':
      return {
        kicker: `Imported from ${adapterDisplayName(source.adapterId)}`,
        title: source.snapshot.title,
        meta: adapterDisplayName(source.adapterId),
      }
    case 'checkFailure':
      return {
        kicker: 'Started from a failed check',
        title: source.snapshot.title,
        meta: source.provider,
      }
  }
}

/**
 * Shows what started this session, above the transcript, per design.md
 * "Message history" / product-brief "The conversation shell": the source
 * stays visible for the life of the chat, using the cached snapshot when the
 * live thing has changed or is gone.
 *
 * Anatomy follows the real mockup's `.ag-source` block (icon button, kicker
 * line, title, meta line, trailing state) rather than the single-pill
 * placeholder this replaces -- see mockup's `data-open-source` button and
 * `.ag-source-state` sibling.
 */
export function SessionSourceBanner({
  sessionId,
  source,
  state,
  onOpenSource,
}: {
  /** Needed to ask the engine whether this chat may change files. */
  sessionId: string
  source: SessionSource
  state: SessionState
  /** Opens the live source (issue/PR/OpenSpec item) this chat started from. */
  onOpenSource?: () => void
}) {
  const { kicker, title, meta } = describeSource(source)
  const Icon = SOURCE_KIND_ICON[source.kind] ?? MessageSquareText
  const liveUnavailable = source.kind !== 'manual' && source.snapshot.liveUnavailable
  // The banner is one dense row, so the age rides as a tooltip rather than a
  // second line -- the full sentence lives in the Source panel.
  const freshness =
    source.kind === 'manual' ? null : describeSnapshotFreshness(source.snapshot.capturedAt, liveUnavailable)
  const missingSource = state === 'missingSource'
  // "Read-only review" is a claim about what the agent may do, so it comes
  // from the engine's own tool gate rather than from the source kind.
  //
  // This used to read `pullRequest || commit || diff`, which is not the
  // question the engine asks at all -- it decides from the chat's intent and
  // whether a plan has started. The two agreed only because of which kickoffs
  // happen to exist today, and issue kickoffs already accept `fix`. The
  // provider picker's own comment records this exact rule being re-derived in
  // the UI once before and disagreeing with the engine.
  //
  // `null` while unknown: the label simply does not claim read-only until it
  // has an answer, rather than guessing in either direction.
  const readOnly = useSessionReadOnly(sessionId)

  return (
    <div className="flex flex-none items-center gap-2 border-b border-border bg-panel2 px-3 py-1.5">
      <button
        type="button"
        onClick={onOpenSource}
        disabled={!onOpenSource}
        aria-label="Open the source for this chat"
        className="flex min-w-0 flex-1 items-center gap-2 rounded px-1 py-0.5 text-left hover:bg-panel3 disabled:cursor-default disabled:hover:bg-transparent"
      >
        <Icon size={16} className="flex-none text-muted-foreground" aria-label={sourceKindLabel(source.kind)} />
        <span className="flex min-w-0 flex-col leading-tight">
          <span className="text-2xs font-bold uppercase tracking-wide text-accent-text">{kicker}</span>
          <span className="truncate text-xs font-semibold text-foreground">{title}</span>
          <span className="truncate font-mono text-2xs text-sub">{meta}</span>
        </span>
      </button>

      <div className="flex flex-none items-center gap-1.5 text-2xs">
        {missingSource ? (
          <span className="font-semibold text-[var(--gw-red)]">Could not load this source</span>
        ) : liveUnavailable ? (
          <span className="font-semibold text-[var(--gw-amber)]" title={freshness ?? undefined}>
            No longer available
          </span>
        ) : readOnly === true ? (
          <span className="font-semibold text-muted-foreground" title={freshness ?? undefined}>
            Read-only review
          </span>
        ) : readOnly === null ? (
          // Not known yet. Says nothing about permissions rather than falling
          // through to "Live source", which asserts the chat CAN change files
          // -- the wrong direction to guess on a safety label.
          <span className="font-semibold text-muted-foreground" title={freshness ?? undefined}>
            Source
          </span>
        ) : (
          <span className="font-semibold text-accent-text" title={freshness ?? undefined}>
            Live source
          </span>
        )}
      </div>
    </div>
  )
}
