import { useState } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import { toast } from 'sonner'
import { Blocks, FolderGit2, GitBranch, Layers3, Link2, Loader2, TriangleAlert } from 'lucide-react'
import { commands, type AgentSession } from '@/lib/bindings'
import { keys, unwrap } from '@/lib/queryKeys'
import { sourceKindLabel } from '@/lib/agentSessionGrouping'
import { adapterDisplayName } from '@/lib/agentImportDisplay'
import { explainRefreshSourceOutcome } from '@/lib/agentDeskSources'
import { describeError, log } from '@/lib/log'
import { useOpenSpecContextDrift } from '@/hooks/useOpenspecSessionSource'
import { SessionUsageCard } from './SessionUsageCard'

/**
 * A backslash, built from its character code. Written this way because a
 * literal backslash in a regex does not reliably survive the tooling that
 * edits this file, and a silently-wrong separator would leave Windows paths
 * unsplit while Unix ones looked fine.
 */
const BACKSLASH = String.fromCharCode(92)

/**
 * The last two segments of a path, which is the part a person recognises.
 *
 * The context card puts its value in the bold slot, so a full Windows path was
 * the most prominent text in the card and truncated from the right, which
 * keeps the drive letter and hides the folder. The whole path is on hover.
 */
function shortPath(path: string): string {
  const parts = path.split(BACKSLASH).join('/').split('/').filter(Boolean)
  if (parts.length <= 2) return path
  return `…/${parts.slice(-2).join('/')}`
}

/** One "label / value" row, matching the mockup's `.ag-context-row`. */
function ContextRow({
  icon: Icon,
  label,
  value,
}: {
  icon: React.ComponentType<{ size?: number; className?: string; 'aria-hidden'?: boolean }>
  label: string
  value: string
}) {
  return (
    <div className="flex items-center gap-2 border-t border-border px-2 py-1.5 first:border-t-0">
      <Icon size={13} className="flex-none text-muted-foreground" aria-hidden />
      <strong
        className="min-w-0 flex-1 truncate text-2xs font-semibold text-foreground"
        // The full value on hover: the row truncates from the right, which for
        // a filesystem path hides the folder name and keeps the drive letter --
        // the least useful half.
        title={value}
      >
        {value}
      </strong>
      <span className="flex-none text-2xs text-muted-foreground">{label}</span>
    </div>
  )
}

/** A short, human description of what a session's source points at. */
function sourceSummary(session: AgentSession): string {
  const { source } = session.header
  switch (source.kind) {
    case 'manual':
      return 'Manual chat'
    case 'issue':
      return `Issue #${source.number}`
    case 'pullRequest':
      return `Pull request #${source.number}`
    case 'openSpecChange':
      return `OpenSpec change ${source.changeId}`
    case 'openSpecTask':
      return `${source.changeId} · task ${source.taskIndex + 1}`
    case 'commit':
      return `Commit ${source.oid.slice(0, 10)}`
    case 'diff':
      return `Diff · ${source.paths.length} file${source.paths.length === 1 ? '' : 's'}`
    case 'workingChanges':
      return `Working changes · ${source.paths.length} file${source.paths.length === 1 ? '' : 's'}`
    case 'checkFailure':
      return `${sourceKindLabel(source.kind)} · ${source.provider}`
    case 'imported':
      return `${sourceKindLabel(source.kind)} · ${adapterDisplayName(source.adapterId)}`
  }
}

/**
 * The Context tab (tasks.md 7.1/7.2): project, source, and context-source
 * counts for the selected session, plus the collapsible usage card.
 *
 * Structure follows the mockup's `.ag-context-view` (a stack of
 * `.ag-context-card` sections) -- see `docs/agent-desk/agent-desk-mockup.html`
 * lines ~2007-2023. Branch/worktree is not shown: `AgentSessionHeader` (see
 * `src/lib/bindings.ts`) carries `repoId`/`repoPath`/`repoName` but no
 * branch or worktree name yet, so rather than guess or fabricate one this
 * panel only shows what the session data actually has -- the same
 * never-invent-a-value rule the usage card follows for provider data.
 */
export function SessionContextPanel({ session }: { session: AgentSession }) {
  const contextSourceCount = session.attachments.length
  const isOpenSpecSource =
    session.header.source.kind === 'openSpecChange' || session.header.source.kind === 'openSpecTask'
  // tasks.md 2.4, third of three ("mark launch-vs-live differences"): a
  // session started from OpenSpec is compared against the CURRENT change
  // files on every poll -- see `useOpenSpecContextDrift`'s own doc comment
  // for why polling rather than the file watcher today.
  const drift = useOpenSpecContextDrift(session.header.sessionId, isOpenSpecSource)
  const diverged = drift.data?.kind === 'checked' && drift.data.diverged
  const qc = useQueryClient()
  const [refreshing, setRefreshing] = useState(false)

  // The banner used to name this action without offering it -- the command
  // existed and had no button anywhere in the app, which reads to a beginner
  // as their own mistake rather than a missing control.
  const refreshSource = async () => {
    if (refreshing) return
    setRefreshing(true)
    try {
      const outcome = unwrap(await commands.agentSessionRefreshSource(session.header.sessionId))
      const { message, ok } = explainRefreshSourceOutcome(outcome)
      if (ok) toast.success(message)
      else toast.warning(message)
      void qc.invalidateQueries({ queryKey: keys.agentSession(session.header.sessionId) })
      void drift.refetch()
    } catch (e) {
      const detail = describeError(e)
      log.error(`agent desk: could not refresh the session source: ${detail}`)
      toast.error('Could not refresh the source.', { description: detail })
    } finally {
      setRefreshing(false)
    }
  }

  return (
    <div className="flex flex-col gap-2">
      {diverged && (
        <div className="flex items-start gap-1.5 rounded-md border border-[var(--gw-amber)]/40 bg-[var(--gw-amber)]/10 px-2 py-1.5 text-2xs leading-relaxed text-[var(--gw-amber)]">
          <TriangleAlert size={12} className="mt-px flex-none" aria-hidden />
          <div className="flex min-w-0 flex-1 flex-col gap-1.5">
            <span>
              The spec changed since the last time an agent read it. Refresh it below, or just send a message -- the
              next turn reads the current files either way.
            </span>
            <button
              type="button"
              onClick={() => void refreshSource()}
              disabled={refreshing}
              className="inline-flex w-fit items-center gap-1 rounded border border-[var(--gw-amber)]/50 px-1.5 py-0.5 text-2xs font-semibold hover:bg-[var(--gw-amber)]/15 disabled:opacity-60"
            >
              {refreshing ? <Loader2 size={11} className="animate-spin motion-reduce:animate-none" aria-hidden /> : null}
              {refreshing ? 'Refreshing…' : 'Refresh the source'}
            </button>
          </div>
        </div>
      )}

      <section className="rounded-md border border-border bg-panel2">
        <ContextRow icon={FolderGit2} label="project" value={session.header.repoName} />
        {/*
          Shown as "…\parentolder" rather than the whole path. The bold slot
          is the most prominent text in the card, and a full `C:\...` path
          truncated from the right shows the drive letter and hides the folder
          -- machine text in a person-facing position. The full path is on
          hover, and the project's own name is the row above.
        */}
        <ContextRow icon={GitBranch} label="folder" value={shortPath(session.header.repoPath)} />
        <ContextRow icon={Link2} label="source" value={sourceSummary(session)} />
      </section>

      <SessionUsageCard sessionId={session.header.sessionId} />

      <section className="rounded-md border border-border bg-panel2">
        <ContextRow icon={Layers3} label="Context sources" value={String(contextSourceCount)} />
        <ContextRow icon={Blocks} label="Conversation segments" value={String(session.segments.length)} />
      </section>
    </div>
  )
}
