import { Blocks, FolderGit2, GitBranch, Layers3, Link2, TriangleAlert } from 'lucide-react'
import type { AgentSession } from '@/lib/bindings'
import { sourceKindLabel } from '@/lib/agentSessionGrouping'
import { useOpenSpecContextDrift } from '@/hooks/useOpenspecSessionSource'
import { SessionUsageCard } from './SessionUsageCard'

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
      <strong className="min-w-0 flex-1 truncate text-2xs font-semibold text-foreground">{value}</strong>
      <span className="flex-none text-[10px] text-muted-foreground">{label}</span>
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

  return (
    <div className="flex flex-col gap-2">
      {diverged && (
        <div className="flex items-start gap-1.5 rounded-md border border-amber-600/40 bg-amber-500/10 px-2 py-1.5 text-2xs leading-relaxed text-amber-700 dark:text-amber-400">
          <TriangleAlert size={12} className="mt-px flex-none" aria-hidden />
          <span>
            The OpenSpec source changed since the last time an agent read it. Refresh the source,
            or send a message so the next turn reads the current files.
          </span>
        </div>
      )}

      <section className="rounded-md border border-border bg-panel2">
        <ContextRow icon={FolderGit2} label="project" value={session.header.repoName} />
        <ContextRow icon={GitBranch} label="repository" value={session.header.repoPath} />
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
