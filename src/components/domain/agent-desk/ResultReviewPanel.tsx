import { useState } from 'react'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { toast } from 'sonner'
import { CheckCircle2, CircleAlert, ExternalLink, FileDiff, GitCommitHorizontal, RotateCcw, Wrench } from 'lucide-react'
import { commands, type ResultRecord, type SessionIntent } from '@/lib/bindings'
import { invalidateAfterResultLanding, keys, unwrap } from '@/lib/queryKeys'
import { useCompleteOpenSpecTask } from '@/hooks/useOpenspecSessionSource'
import { useAgentSessionMutations } from '@/hooks/useAgentSessionMutations'
import { useAgentDeskUiStore } from '@/stores/agentDeskUiStore'
import {
  canEscalateToFix,
  changedPathsSummaryLine,
  checksSummaryLine,
  explainCommitOutcome,
  explainCompleteOpenSpecTaskOutcome,
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
 *
 * `isOpenSpecTask` is P1-C wiring 1 ("the OpenSpec completion hook has no
 * consumer"): `commands::agent_desk::agent_session_complete_openspec_task`
 * existed, was registered, and had a passing test suite, but no UI ever
 * called it -- accepting a result from a session started on an OpenSpec
 * task never actually checked that task off in `tasks.md`. Keep (this
 * panel's "the changes are good" gesture -- `CompleteOpenSpecTaskOutcome`'s
 * own doc comment: "requires the caller to have already gotten that
 * acceptance ... rather than inferring it from execution state") is that
 * gesture, so `handleKeep` fires the completion call right alongside
 * `agentResultKeep` whenever this session's source is an OpenSpec task.
 * Sessions from any other source pass `false` and this never fires --
 * exactly what `CompleteOpenSpecTaskOutcome::NotAnOpenSpecTaskSource` exists
 * to refuse anyway, so the frontend gate is belt-and-suspenders, not the
 * only thing stopping a bad call.
 */
export function ResultReviewPanel({
  sessionId,
  repoId,
  executionId,
  intent,
  taskText,
  provider,
  isOpenSpecTask = false,
}: {
  sessionId: string
  /** The repository the result lands in; its views refresh after Keep/Undo/Commit. */
  repoId: string
  executionId: string
  intent: SessionIntent
  taskText: string
  provider: string
  isOpenSpecTask?: boolean
}) {
  const queryClient = useQueryClient()
  const [commitMessage, setCommitMessage] = useState<string | null>(null)
  // P1-C wiring 2 ("Request revision changes state but does not append
  // guidance or start a turn"): `null` means the guidance box is closed;
  // `''`/text means it is open and the user is typing what needs to
  // change. Mirrors `commitMessage`'s own null-means-closed convention just
  // above.
  const [revisionText, setRevisionText] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const query = useQuery({
    queryKey: keys.agentResults(sessionId),
    queryFn: async () => unwrap(await commands.agentResultList(sessionId)),
  })

  const records = query.data?.kind === 'found' ? sortResultsNewestFirst(query.data.records) : []
  const record = records.find((r) => r.executionId === executionId) ?? null

  const completeTask = useCompleteOpenSpecTask(repoId)
  const { escalateToFix } = useAgentSessionMutations()
  // Which pane is showing this review. `ConversationPane` does not pass its
  // pane id down, but the layout knows which session each pane holds, so
  // the pane can be recovered from `sessionId`. Read here (not in the
  // handler) so the lookup is a subscription, not a stale closure.
  const secondarySessionId = useAgentDeskUiStore((s) => s.layout.secondarySessionId)
  const setPaneSession = useAgentDeskUiStore((s) => s.setPaneSession)
  const setActivePane = useAgentDeskUiStore((s) => s.setActivePane)
  const refresh = () => {
    invalidateAfterResultLanding(queryClient, repoId, sessionId)
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

      // P1-C wiring 1: Keep is this panel's "accepted" gesture -- check the
      // originating OpenSpec task off in the same breath, for sessions that
      // came from one. Deliberately fire-and-forget-with-its-own-toast
      // rather than blocking the Keep success above on it: the result was
      // genuinely kept either way, and a task-list write failure is a
      // separate, secondary thing to tell the user about, not a reason to
      // make Keep itself look like it failed.
      if (isOpenSpecTask) {
        try {
          // Through the mutation hook, not the raw command: the hook is what
          // refreshes the task list and progress surfaces once the box is
          // ticked. Calling the command directly left them stale.
          const taskOutcome = await completeTask.mutateAsync({ sessionId, done: true })
          const taskExplanation = explainCompleteOpenSpecTaskOutcome(taskOutcome)
          if (taskExplanation) toast.warning(taskExplanation)
        } catch (e) {
          log.error(`could not check off the OpenSpec task after Keep: ${describeError(e)}`)
          toast.warning('Kept, but the task list could not be updated. Check it off by hand.')
        }
      }
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

  /** Opens the guidance box -- the actual revision request is submitted by
   * `handleSubmitRevision` once the user has written what needs to change.
   * Splitting "start requesting a revision" from "submit it" mirrors the
   * commit-message flow's `handleDraftCommitMessage`/`handleCommit` split
   * just below, and is why this is not itself wrapped in `withBusy`: opening
   * a text box has nothing to be busy about. */
  function handleRequestRevision() {
    setRevisionText('')
  }

  /**
   * P1-C wiring 2: what `request_revision_at`'s own doc comment says the
   * caller is responsible for, done here as the three real steps in order --
   * `agentResultRequestRevision`'s doc comment is explicit that flipping the
   * result's state was never meant to be the whole feature:
   *
   *   1. Flip the result's state to `RevisionRequested` (what the backend
   *      command already did, in isolation, before this wiring existed).
   *   2. Append the user's guidance as a real transcript message, so the
   *      lead sees exactly what changed request -- not a bare re-run with
   *      no new instruction.
   *   3. Start a new execution on the same session/mode/team the ordinary
   *      Send button would use, so "Review requested changes" genuinely
   *      continues the conversation instead of leaving the user to
   *      separately go find the composer and repeat themselves.
   *
   * A failure at step 2 or 3 still leaves the result correctly marked
   * `RevisionRequested` (step 1 already committed) -- the guidance and/or
   * turn simply did not go out, and the toast says so plainly rather than
   * claiming a turn started when it did not (the same "never claim delivery
   * that did not happen" rule `SessionComposer`'s own `alreadyRunning`
   * branch follows).
   */
  async function handleSubmitRevision() {
    const guidance = (revisionText ?? '').trim()
    if (!guidance) {
      toast.error('Write what needs to change first.')
      return
    }
    await withBusy(async () => {
      const requestOutcome = unwrap(await commands.agentResultRequestRevision(sessionId, executionId))
      if (requestOutcome.kind !== 'requested') {
        toast.error('Could not mark this result for revision.', { description: requestOutcome.kind })
        return
      }
      refresh()

      try {
        const appended = unwrap(await commands.agentSessionAppendUserMessage(sessionId, guidance, []))
        if (appended.kind !== 'appended') {
          toast.error('Marked for revision, but your guidance could not be saved.', { description: appended.kind })
          return
        }
        setRevisionText(null)

        // Reads the real policy table (`agentdesk::policy::for_intent`)
        // rather than hand-copying "auto"/"lead" here -- same reasoning
        // `useStartAgentSession`'s own doc comment gives for doing the
        // equivalent lookup: it is what keeps this in sync with the
        // backend's table instead of silently drifting from it.
        const policy = await commands.agentIntentPolicy(intent)
        const startOutcome = unwrap(
          await commands.agentSessionStartExecution(sessionId, policy.defaultMode, policy.defaultTeam, null)
        )
        if (startOutcome.kind === 'started') {
          toast.success('Sent -- the lead is working on your requested changes.')
        } else if (startOutcome.kind === 'alreadyRunning') {
          // Same "do not claim delivery that did not happen" stance
          // `SessionComposer` takes: the guidance is saved and visible, but
          // this specific call did not start a new turn for it.
          toast.info('Saved for the next turn. The agent is still finishing its current one.')
        } else {
          toast.error('Your guidance was saved, but the agent could not start.', { description: startOutcome.kind })
        }
      } catch (e) {
        log.error(`could not continue after requesting revision: ${describeError(e)}`)
        toast.error('Marked for revision, but could not send your guidance. Send it from the composer instead.')
      }
    })
  }

  /**
   * Source-kickoffs task 4.6: a Review/Explain/Summarize/Ask chat is
   * read-only by design. When its conclusion is worth acting on, this opens
   * a NEW Fix chat on the same source, seeded with that conclusion, and
   * lands the person in it. Nothing runs until they press Send -- the seed
   * is theirs to read and edit first, which is why the toast tells them to
   * do exactly that. Refusals are already toasted by the mutation hook.
   */
  async function handleFixThis() {
    await withBusy(async () => {
      let outcome
      try {
        outcome = await escalateToFix.mutateAsync(sessionId)
      } catch {
        // The hook's onError already logged and toasted this; a second
        // "something went wrong" from withBusy would just be noise.
        return
      }
      if (outcome.kind !== 'created') return
      const pane = secondarySessionId === sessionId ? 'secondary' : 'primary'
      setPaneSession(pane, outcome.session.header.sessionId)
      setActivePane(pane)
      toast.success('Started a fix chat from this review.', {
        description: 'Read the message, then press Send.',
      })
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

      {revisionText !== null && availability.canRequestRevision && (
        <div className="flex flex-col gap-1.5">
          <textarea
            value={revisionText}
            onChange={(e) => setRevisionText(e.target.value)}
            rows={4}
            autoFocus
            className="w-full resize-none rounded-md border border-border bg-panel2 p-2 text-2xs text-foreground"
            placeholder="What needs to change?"
          />
        </div>
      )}

      <div className="flex flex-wrap gap-1.5">
        {canEscalateToFix(intent) && (
          <ActionButton
            onClick={() => void handleFixThis()}
            disabled={busy}
            variant="primary"
            icon={<Wrench size={12} aria-hidden />}
          >
            Fix this
          </ActionButton>
        )}
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
        {availability.canRequestRevision && revisionText === null && (
          <ActionButton onClick={handleRequestRevision} disabled={busy} variant="ghost">
            Review requested changes
          </ActionButton>
        )}
        {availability.canRequestRevision && revisionText !== null && (
          <>
            <ActionButton onClick={() => void handleSubmitRevision()} disabled={busy} variant="primary">
              Send to the lead
            </ActionButton>
            <ActionButton onClick={() => setRevisionText(null)} disabled={busy} variant="ghost">
              Cancel
            </ActionButton>
          </>
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
