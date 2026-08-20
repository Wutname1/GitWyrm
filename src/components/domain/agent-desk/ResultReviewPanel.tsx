import { useState } from 'react'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { toast } from 'sonner'
import { CheckCircle2, CircleAlert, ExternalLink, FileDiff, GitCommitHorizontal, RotateCcw } from 'lucide-react'
import { commands, type ResultRecord, type SessionIntent } from '@/lib/bindings'
import { keys, unwrap } from '@/lib/queryKeys'
import {
  changedPathsSummaryLine,
  checksSummaryLine,
  explainCommitOutcome,
  explainKeepOutcome,
  explainUndoOutcome,
  hasFailingCheck,
  resultActionAvailability,
  resultStateLabel,
  sortResultsNewestFirst,
} from '@/lib/agentDeskResult'
import { log, describeError } from '@/lib/log'
import { cn } from '@/lib/utils'

/**
 * Review one helper's result, or the combined lead result, in place --
 * agent-desk-review-and-landing tasks.md section 2 and 3. Reads the unified
 * result state (`commands::agent_result::agent_result_list`) and offers
 * Keep/Undo/Request revision/Commit/Draft PR, each routed through the
 * matching command so the actual work stays in the backend's existing
 * run-completion/commit/worktree machinery (this component only displays
 * outcomes and asks for confirmation copy -- it never touches git itself).
 *
 * `sessionId` names the session; `executionId` selects which result to show
 * (the lead's own execution id for the combined result, or a helper's for
 * its scoped one -- tasks.md 2.2's "graph node Output/View diff open that
 * helper-scoped result").
 */
export function ResultReviewPanel({
  sessionId,
  executionId,
  intent,
  taskText,
  provider,
}: {
  sessionId: string
  executionId: string
  intent: SessionIntent
  taskText: string
  provider: string
}) {
  const queryClient = useQueryClient()
  const [commitMessage, setCommitMessage] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const query = useQuery({
    queryKey: keys.agentResults(sessionId),
    queryFn: async () => unwrap(await commands.agentResultList(sessionId)),
  })

  const records = query.data?.kind === 'found' ? sortResultsNewestFirst(query.data.records) : []
  const record = records.find((r) => r.executionId === executionId) ?? null

  const refresh = () => {
    void queryClient.invalidateQueries({ queryKey: keys.agentResults(sessionId) })
  }

  if (query.isLoading) {
    return <div className="p-3 text-xs text-muted-foreground">Loading result…</div>
  }
  if (!record) {
    return <div className="p-3 text-xs text-muted-foreground">No result yet for this execution.</div>
  }

  const availability = resultActionAvailability(record)
  const changedLine = changedPathsSummaryLine(record.changedPaths)
  const checksLine = checksSummaryLine(record.checks)
  const failingChecks = hasFailingCheck(record.checks)

  async function withBusy(fn: () => Promise<void>) {
    setBusy(true)
    try {
      await fn()
    } catch (e) {
      log.error(`result review action failed: ${describeError(e)}`)
      toast.error('Something went wrong. Try again.')
    } finally {
      setBusy(false)
    }
  }

  async function handleKeep() {
    await withBusy(async () => {
      const outcome = unwrap(await commands.agentResultKeep(sessionId, executionId))
      const explanation = explainKeepOutcome(outcome)
      if (explanation) {
        toast.error(explanation)
        return
      }
      toast.success('Kept -- ready to commit when you are.')
      refresh()
    })
  }

  async function handleUndo() {
    await withBusy(async () => {
      const outcome = unwrap(await commands.agentResultUndo(sessionId, executionId))
      const { message, refusedHandEdited } = explainUndoOutcome(outcome)
      if (message) {
        if (refusedHandEdited) {
          toast.warning(message)
        } else {
          toast.error(message)
        }
        refresh()
        return
      }
      toast.success('Undone -- the changes were discarded.')
      refresh()
    })
  }

  async function handleRequestRevision() {
    await withBusy(async () => {
      unwrap(await commands.agentResultRequestRevision(sessionId, executionId))
      toast.success('Marked for revision. Send the lead a follow-up message to continue.')
      refresh()
    })
  }

  async function handleDraftCommitMessage() {
    await withBusy(async () => {
      const message = unwrap(await commands.agentResultDraftCommitMessage(sessionId, executionId, taskText, provider))
      setCommitMessage(message ?? '')
    })
  }

  async function handleCommit() {
    if (!commitMessage || !commitMessage.trim()) {
      toast.error('Write a commit message first.')
      return
    }
    await withBusy(async () => {
      const outcome = unwrap(await commands.agentResultCommit(sessionId, executionId, intent, commitMessage))
      const explanation = explainCommitOutcome(outcome)
      if (explanation) {
        toast.error(explanation)
        return
      }
      toast.success('Committed.')
      setCommitMessage(null)
      refresh()
    })
  }

  function handleOpenDiff(path?: string) {
    if (!record?.worktreePath) return
    void commands.agentResultOpenDiff(record.worktreePath, path ?? null).then((res) => {
      if (res.status === 'ok' && res.data.kind === 'mainWindowNotOpen') {
        toast.error('Open the main GitWyrm window first.')
      }
    })
  }

  return (
    <div className="flex flex-col gap-3 p-3">
      <div className="flex items-center justify-between gap-2">
        <span
          className={cn(
            'inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-2xs font-medium',
            record.state === 'committed' && 'bg-success/15 text-success',
            record.state === 'kept' && 'bg-accent/15 text-accent',
            (record.state === 'reviewing' || record.state === 'revisionRequested') && 'bg-muted text-muted-foreground',
            (record.state === 'discarded' || record.state === 'cleanupFailed') && 'bg-destructive/15 text-destructive',
          )}
        >
          {resultStateLabel(record.state)}
        </span>
        {record.commit && (
          <span className="flex items-center gap-1 text-2xs text-muted-foreground">
            <GitCommitHorizontal size={12} aria-hidden />
            {record.commit.oid.slice(0, 7)}
          </span>
        )}
      </div>

      <div className="rounded-md border border-border bg-panel2 p-2">
        <div className="flex items-center justify-between">
          <span className="text-xs font-medium text-foreground">{changedLine}</span>
          {record.worktreePath && record.changedPaths.length > 0 && (
            <button
              type="button"
              onClick={() => handleOpenDiff()}
              className="flex items-center gap-1 text-2xs text-accent hover:underline"
            >
              <FileDiff size={12} aria-hidden />
              View diff
            </button>
          )}
        </div>
        {record.changedPaths.length > 0 && (
          <ul className="mt-1.5 flex flex-col gap-0.5">
            {record.changedPaths.slice(0, 50).map((p) => (
              <li key={p.path} className="flex items-center justify-between gap-2 text-2xs text-muted-foreground">
                <button
                  type="button"
                  onClick={() => handleOpenDiff(p.path)}
                  className="truncate text-left hover:text-foreground hover:underline"
                  title={p.path}
                >
                  {p.oldPath ? `${p.oldPath} → ${p.path}` : p.path}
                </button>
                <span className="flex-none font-mono">{p.status}</span>
              </li>
            ))}
            {record.changedPaths.length > 50 && (
              <li className="text-2xs text-muted-foreground">…and {record.changedPaths.length - 50} more</li>
            )}
          </ul>
        )}
      </div>

      {checksLine && (
        <div className="flex items-center gap-1.5 rounded-md border border-border bg-panel2 px-2 py-1.5 text-2xs">
          {failingChecks ? (
            <CircleAlert size={12} className="flex-none text-destructive" aria-hidden />
          ) : (
            <CheckCircle2 size={12} className="flex-none text-success" aria-hidden />
          )}
          <span className={cn('text-muted-foreground', failingChecks && 'text-destructive')}>{checksLine}</span>
        </div>
      )}

      {commitMessage !== null && availability.canCommit && (
        <div className="flex flex-col gap-1.5">
          <textarea
            value={commitMessage}
            onChange={(e) => setCommitMessage(e.target.value)}
            rows={4}
            className="w-full resize-none rounded-md border border-border bg-panel2 p-2 font-mono text-2xs text-foreground"
            placeholder="Commit message"
          />
        </div>
      )}

      <div className="flex flex-wrap gap-1.5">
        {availability.canKeep && (
          <ActionButton onClick={handleKeep} disabled={busy}>
            Keep
          </ActionButton>
        )}
        {availability.canUndo && (
          <ActionButton onClick={handleUndo} disabled={busy} icon={<RotateCcw size={12} aria-hidden />}>
            Undo
          </ActionButton>
        )}
        {availability.canRequestRevision && (
          <ActionButton onClick={handleRequestRevision} disabled={busy} variant="ghost">
            Review requested changes
          </ActionButton>
        )}
        {availability.canCommit && commitMessage === null && (
          <ActionButton onClick={handleDraftCommitMessage} disabled={busy}>
            Prepare commit
          </ActionButton>
        )}
        {availability.canCommit && commitMessage !== null && (
          <ActionButton onClick={handleCommit} disabled={busy} variant="primary">
            Commit
          </ActionButton>
        )}
        {availability.canDraftPullRequest && (
          <PullRequestButton sessionId={sessionId} executionId={executionId} disabled={busy} />
        )}
      </div>
    </div>
  )
}

function ActionButton({
  onClick,
  disabled,
  children,
  icon,
  variant = 'default',
}: {
  onClick: () => void
  disabled?: boolean
  children: React.ReactNode
  icon?: React.ReactNode
  variant?: 'default' | 'primary' | 'ghost'
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      className={cn(
        'inline-flex items-center gap-1 rounded-md border px-2.5 py-1 text-2xs font-medium disabled:cursor-not-allowed disabled:opacity-50',
        variant === 'primary' && 'border-accent bg-accent text-accent-foreground hover:bg-accent/90',
        variant === 'default' && 'border-border bg-panel2 text-foreground hover:bg-panel3',
        variant === 'ghost' && 'border-transparent text-muted-foreground hover:text-foreground',
      )}
    >
      {icon}
      {children}
    </button>
  )
}

/**
 * Task 4: PR creation is a separate explicit action from Commit, and never
 * pushes -- it drafts editable title/body and, only on a second explicit
 * click, opens the host's own compare/new-PR page in the browser. GitWyrm
 * never calls a host write API or `git_push` from this button.
 */
function PullRequestButton({ sessionId, executionId, disabled }: { sessionId: string; executionId: string; disabled?: boolean }) {
  const [drafting, setDrafting] = useState(false)

  async function handleDraft() {
    setDrafting(true)
    try {
      const outcome = unwrap(await commands.agentResultDraftPullRequest(sessionId, executionId))
      if (outcome.kind !== 'drafted') {
        toast.error('Could not prepare a pull request draft for this result.')
        return
      }
      if (!outcome.draft.compareUrl) {
        toast.error('This repository is not connected to a host GitWyrm recognizes.')
        return
      }
      toast('Review the pull request before it opens', {
        description: outcome.draft.title,
        action: {
          label: 'Open on host',
          onClick: () => {
            void commands.agentResultOpenPullRequestPage(outcome.draft.compareUrl!)
          },
        },
      })
    } catch (e) {
      log.error(`draft pull request failed: ${describeError(e)}`)
      toast.error('Something went wrong preparing the pull request.')
    } finally {
      setDrafting(false)
    }
  }

  return (
    <ActionButton onClick={() => void handleDraft()} disabled={disabled || drafting} icon={<ExternalLink size={12} aria-hidden />}>
      Create pull request
    </ActionButton>
  )
}
