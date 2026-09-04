import { useEffect, useState } from 'react'
import { toast } from 'sonner'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { FileDiff, GitFork } from 'lucide-react'
import { commands, type AgentSession, type ExecutionRecord, type ResultRecord } from '@/lib/bindings'
import { keys, unwrap } from '@/lib/queryKeys'
import { describeError, log } from '@/lib/log'
import { nodeUsageLine } from '@/lib/agentDeskUsage'
import { describeOutcome, explainStopOutcome, runIsActive, runStoppedBadly } from '@/lib/agentDeskResult'
import { cn } from '@/lib/utils'
import { buildGraphTree, graphSummary, nodeDotTone, nodeStatusLabel, type GraphTreeNode } from '@/lib/agentGraphProjection'
import { canViewNodeChanges, helperRoleLabel, latestActivityLine, resultForNode, revisionSeed } from '@/lib/agentDeskGraph'
import { useAgentDeskUiStore } from '@/stores/agentDeskUiStore'
import { AwaitingStartCard } from './AwaitingStartCard'
import { ResultReviewPanel } from './ResultReviewPanel'

// Every label `nodeStatusLabel` can emit has a tone. The two that were
// missing -- 'source missing' and 'stopped early' -- fell through to muted
// grey, which made a dead agent render CALMER than a working one and let a
// person scanning the tree miss it entirely.
const STATUS_TONE: Record<string, string> = {
  working: 'text-accent-text',
  starting: 'text-accent-text',
  waiting: 'text-[var(--gw-amber)]',
  conflict: 'text-[var(--gw-amber)]',
  failed: 'text-[var(--gw-red)]',
  'source missing': 'text-[var(--gw-red)]',
  stopped: 'text-muted-foreground',
  'stopped early': 'text-[var(--gw-amber)]',
  done: 'text-added',
  queued: 'text-muted-foreground',
  'not started': 'text-muted-foreground',
  ready: 'text-muted-foreground',
}

const DOT_TONE: Record<string, string> = {
  lead: 'bg-added',
  done: 'bg-added',
  working: 'bg-accent-text animate-pulse motion-reduce:animate-none',
  waiting: 'bg-[var(--gw-amber)]',
  attention: 'bg-[var(--gw-red)]',
  interrupted: 'bg-[var(--gw-amber)]',
}

/** One row in the graph tree: an L-shaped connector for helper rows, a
 * status dot, title + meta, the node's latest activity line (tasks.md 6.2:
 * what it is doing right now), and the status word (mockup `.ag-node-wrap`,
 * `.ag-node-dot`, `.ag-node-status`). */
function GraphNodeRow({
  node,
  activity,
  selected,
  onSelect,
}: {
  node: GraphTreeNode
  activity: string | null
  selected: boolean
  onSelect: () => void
}) {
  const { execution, isLead } = node
  const status = nodeStatusLabel(node)
  const tone = nodeDotTone(node)
  const title = isLead ? 'Lead agent' : (execution.jobTitle ?? 'Helper')
  const meta = isLead
    ? 'owns source + integration'
    : [
        // The stored role token ('builder') is not a phrase anyone says.
        execution.helperRole ? helperRoleLabel(execution.helperRole) : undefined,
        execution.worktreePath ? 'can change files' : 'reads only',
      ]
        .filter(Boolean)
        .join(' · ')

  return (
    <div className={cn('relative', !isLead && 'pl-6')}>
      {!isLead && (
        <span
          aria-hidden
          className="pointer-events-none absolute left-2.5 top-0 h-1/2 w-3 border-b border-l border-border"
        />
      )}
      <button
        type="button"
        aria-pressed={selected}
        onClick={onSelect}
        className={cn(
          'flex w-full items-center gap-2 rounded-md border px-2 py-1.5 text-left transition-colors',
          // `border-accent` was the selected edge, but `--accent` resolves to
          // the same hex as `--border`, so a selected node had literally the
          // same border as an unselected one -- tint alone, which the house
          // Selected Must Read rule forbids. `SessionRow` already does this
          // properly, so this matches it.
          selected ? 'border-primary bg-panel3' : 'border-border bg-panel hover:bg-panel3'
        )}
      >
        <span
          className={cn(
            'h-2.5 w-2.5 flex-none rounded-full',
            tone ? DOT_TONE[tone] : 'bg-muted-foreground/50',
            isLead && 'rounded-[3px_6px_3px_6px]'
          )}
          aria-hidden
        />
        <span className="min-w-0 flex-1">
          <strong className="block truncate text-2xs font-semibold text-foreground">{title}</strong>
          <span className="block truncate text-2xs text-muted-foreground">{meta || 'agent'}</span>
          {activity ? (
            <span className="block truncate text-2xs text-foreground/80" title={activity}>
              {activity}
            </span>
          ) : null}
        </span>
        <span className={cn('flex-none text-2xs font-semibold', STATUS_TONE[status] ?? 'text-muted-foreground')}>
          {status}
        </span>
      </button>
    </div>
  )
}

/** The persistent inspector card (mockup `.ag-inspector`): kicker, title,
 * description, files line, latest activity, and -- once this node has a
 * result -- View changes / Read output (tasks.md 6.2, review tasks.md 2.2
 * and 2.6), plus Stop agent. The result controls are simply absent until
 * `record` exists, never shown disabled. */
function InspectorCard({
  node,
  activity,
  record,
  session,
  onStopped,
}: {
  node: GraphTreeNode
  activity: string | null
  record: ResultRecord | null
  session: AgentSession
  onStopped: () => void
}) {
  const qc = useQueryClient()
  const [stopping, setStopping] = useState(false)
  const [showOutput, setShowOutput] = useState(false)
  const { execution, isLead } = node
  const canViewChanges = canViewNodeChanges(record)

  // Reuses the review-and-landing bridge (`agent_result_open_diff`): the
  // main window opens this helper's worktree as a repo tab and shows its
  // diff there. No diff viewer of its own in this window.
  const viewChanges = async () => {
    if (!record?.worktreePath) return
    try {
      const outcome = unwrap(await commands.agentResultOpenDiff(record.worktreePath, null))
      if (outcome.kind === 'opened') {
        toast.success('Showing the changes in the main GitWyrm window.')
      } else {
        toast.error('Open the main GitWyrm window first.')
      }
    } catch (e) {
      const message = describeError(e)
      log.error(`agent desk: could not open changes for ${execution.executionId}: ${message}`)
      toast.error('Could not open the changes.', { description: message })
    }
  }
  const title = isLead ? 'Lead agent' : (execution.jobTitle ?? 'Helper')
  const description = isLead
    ? 'Owns the conversation, the source, and reviews every helper result before answering.'
    : (execution.jobDescription ?? 'No description yet.')
  const changedFileCount = execution.changedFileCount ?? 0
  const filesLine = isLead
    ? undefined
    : [
        execution.helperRole ? helperRoleLabel(execution.helperRole) : undefined,
        changedFileCount > 0 ? `${changedFileCount} file${changedFileCount === 1 ? '' : 's'} changed` : undefined,
      ]
        .filter(Boolean)
        .join(' · ')
  // What this helper is waiting for. `dependsOn` has been on the record since
  // graphs shipped but was never rendered, so a helper sitting idle because a
  // peer has not finished looked identical to one that had simply stalled --
  // the person had no way to tell "blocked" from "broken". Names are resolved
  // from the session's own executions; an id with no matching record falls
  // back to "another agent" rather than leaking a hex id.
  const waitingFor = (execution.dependsOn ?? [])
    .map((id) => session.executions.find((e) => e.executionId === id)?.jobTitle ?? 'another agent')
    .filter((name, i, all) => all.indexOf(name) === i)
  // Ended without finishing its work, so its `outputSummary` is a reason
  // rather than a result.
  const stoppedBadly = runStoppedBadly(execution.state)
  // What this agent spent. Absent when the provider reported nothing, which
  // is a different fact from "it was free" -- so no line at all rather than a
  // row of zeros.
  const usageLine = nodeUsageLine(execution.usage)
  const canStop = runIsActive(execution.state)
  const [resolving, setResolving] = useState<'helper' | 'integrated' | null>(null)

  // R6.8: preserve both sides of a conflict and resume only the selected
  // integration -- this node's own `conflict` (set by
  // `integrate_helper_result` on the backend) names the base/helper/
  // already-integrated text; picking either keeps the OTHER copy fully
  // intact on disk (it was never overwritten), it just is not the one this
  // node's own result carries forward.
  const resolveConflict = async (resolution: 'keepHelper' | 'keepIntegrated') => {
    if (resolving) return
    setResolving(resolution === 'keepHelper' ? 'helper' : 'integrated')
    try {
      const outcome = unwrap(
        await commands.agentSessionResolveConflict(session.header.sessionId, execution.executionId, {
          kind: resolution,
        })
      )
      if (outcome.kind === 'resolved') {
        void qc.invalidateQueries({ queryKey: keys.agentSession(session.header.sessionId) })
        toast.success(
          resolution === 'keepHelper' ? 'Kept this agent’s version.' : 'Kept the already-integrated version.'
        )
      } else if (outcome.kind === 'noConflict') {
        toast.error('That conflict is already resolved.')
      } else if (outcome.kind === 'notFound') {
        toast.error('This chat is gone. It may have been archived elsewhere.')
      } else if (outcome.kind === 'damaged') {
        toast.error('This chat file is damaged.', { description: outcome.reason })
      } else {
        toast.error('Could not resolve that conflict.', { description: describeOutcome(outcome) })
      }
    } catch (e) {
      const message = describeError(e)
      log.error(`agent desk: could not resolve conflict on ${execution.executionId}: ${message}`)
      toast.error('Could not resolve that conflict.', { description: message })
    } finally {
      setResolving(null)
    }
  }

  const stopThis = async () => {
    if (stopping) return
    setStopping(true)
    try {
      const outcome = unwrap(
        await commands.agentSessionStopExecution(session.header.sessionId, {
          kind: 'one',
          execution_id: execution.executionId,
        })
      )
      if (outcome.kind === 'stopped') {
        void qc.invalidateQueries({ queryKey: keys.agentSession(session.header.sessionId) })
        // `timed_out` is the agent that had to be force-stopped because it
        // never answered. Branching on `stopped` alone reported "That agent
        // already finished." about one that was still running.
        toast.success(
          outcome.stopped.length > 0
            ? isLead
              ? 'Lead agent stopped; work already done was kept.'
              : 'Agent stopped; other work continues.'
            : outcome.timed_out.length > 0
              ? 'That agent did not answer, so it was force-stopped; work already done was kept.'
              : 'That agent already finished.'
        )
        onStopped()
      } else if (outcome.kind === 'notFound') {
        toast.error('This chat is gone. It may have been archived elsewhere.')
      } else if (outcome.kind === 'damaged') {
        toast.error('This chat file is damaged and could not be stopped.', { description: outcome.reason })
      } else {
        toast.error(explainStopOutcome(outcome).message)
      }
    } catch (e) {
      const message = describeError(e)
      log.error(`agent desk: could not stop execution ${execution.executionId}: ${message}`)
      toast.error('Could not stop that agent.', { description: message })
    } finally {
      setStopping(false)
    }
  }

  return (
    <div className="flex-none rounded-md border border-border bg-panel p-2.5">
      <div className="text-2xs font-bold uppercase tracking-wide text-muted-foreground">Selected agent</div>
      <div className="mt-1 text-xs font-semibold text-foreground">{title}</div>
      <p className="mt-1 text-2xs leading-relaxed text-muted-foreground">{description}</p>
      {filesLine ? <div className="mt-1.5 font-mono text-2xs text-muted-foreground">{filesLine}</div> : null}
      {usageLine ? (
        <div className="mt-1.5 font-mono text-2xs text-muted-foreground" title="What this agent has spent so far">
          {usageLine}
        </div>
      ) : null}
      {waitingFor.length > 0 ? (
        <div className="mt-1.5">
          <div className="text-2xs font-bold uppercase tracking-wide text-muted-foreground">Waiting for</div>
          <p className="mt-0.5 text-2xs leading-relaxed text-foreground">
            {waitingFor.join(', ')} to finish first.
          </p>
        </div>
      ) : null}
      {activity ? (
        <div className="mt-1.5">
          <div className="text-2xs font-bold uppercase tracking-wide text-muted-foreground">
            {canStop ? 'Right now' : 'Last activity'}
          </div>
          <p className="mt-0.5 text-2xs leading-relaxed text-foreground">{activity}</p>
        </div>
      ) : null}
      {execution.outputSummary ? (
        // The same field carries a finished agent's result AND a failed one's
        // reason. Under one neutral "Output" heading a failure read exactly
        // like a success, which is the difference the person is looking for.
        <div className="mt-1.5">
          <div
            className={cn(
              'text-2xs font-bold uppercase tracking-wide',
              stoppedBadly ? 'text-[var(--gw-red)]' : 'text-muted-foreground'
            )}
          >
            {stoppedBadly ? 'Why it stopped' : 'Output'}
          </div>
          <p className={cn('mt-0.5 text-2xs leading-relaxed', stoppedBadly ? 'text-foreground' : 'text-muted-foreground')}>
            {execution.outputSummary}
          </p>
        </div>
      ) : null}
      {execution.conflict ? (
        <div className="mt-2 rounded border border-[var(--gw-amber)]/50 bg-panel2 p-2">
          <div className="text-2xs font-bold uppercase tracking-wide text-[var(--gw-amber)]">
            Both versions changed {execution.conflict.path}
          </div>
          <p className="mt-1 text-2xs leading-relaxed text-muted-foreground">
            This agent and another change to the same file both edited it. Nothing was kept automatically -- pick which
            version to carry forward. The other version stays on disk either way.
          </p>
          <div className="mt-2 flex gap-1.5">
            <button
              type="button"
              onClick={() => void resolveConflict('keepHelper')}
              disabled={resolving !== null}
              className="rounded border border-border bg-panel px-1.5 py-1 text-2xs font-semibold text-foreground hover:bg-panel3 disabled:cursor-not-allowed disabled:opacity-50"
            >
              {resolving === 'helper' ? 'Keeping…' : 'Keep this agent’s version'}
            </button>
            <button
              type="button"
              onClick={() => void resolveConflict('keepIntegrated')}
              disabled={resolving !== null}
              className="rounded border border-border bg-panel px-1.5 py-1 text-2xs font-semibold text-foreground hover:bg-panel3 disabled:cursor-not-allowed disabled:opacity-50"
            >
              {resolving === 'integrated' ? 'Keeping…' : 'Keep the other version'}
            </button>
          </div>
        </div>
      ) : null}
      <div className="mt-2 flex flex-wrap gap-1.5">
        {canViewChanges ? (
          <button
            type="button"
            onClick={() => void viewChanges()}
            className="flex items-center gap-1 rounded border border-border bg-panel2 px-1.5 py-1 text-2xs font-semibold text-foreground hover:bg-panel3"
          >
            <FileDiff size={11} aria-hidden />
            View changes
          </button>
        ) : null}
        {record ? (
          <button
            type="button"
            onClick={() => setShowOutput((v) => !v)}
            aria-expanded={showOutput}
            className="rounded border border-border bg-panel2 px-1.5 py-1 text-2xs font-semibold text-foreground hover:bg-panel3"
          >
            {showOutput ? 'Hide output' : 'Read output'}
          </button>
        ) : null}
        <button
          type="button"
          onClick={() => void stopThis()}
          disabled={!canStop || stopping}
          className="rounded border border-border bg-panel2 px-1.5 py-1 text-2xs font-semibold text-foreground hover:bg-panel3 disabled:cursor-not-allowed disabled:opacity-50"
        >
          {stopping ? 'Stopping…' : 'Stop agent'}
        </button>
      </div>
      {record && showOutput ? (
        // The same review surface the conversation shows for a finished
        // run, scoped to THIS node's result (review tasks.md 2.2: "graph
        // node Output/View diff open that helper-scoped result").
        <div className="mt-2 rounded border border-border bg-panel2">
          <ResultReviewPanel
            sessionId={session.header.sessionId}
            repoId={session.header.repoId}
            executionId={execution.executionId}
            intent={session.header.intent}
            taskText={execution.jobDescription ?? execution.jobTitle ?? session.header.title}
            provider={execution.provider ?? 'copilot'}
            isOpenSpecTask={session.header.source.kind === 'openSpecTask'}
          />
        </div>
      ) : null}
    </div>
  )
}

/**
 * The Graph tab (tasks.md 6.1, 6.2, 7.1, 7.5): an indented tree of the
 * session's lead and helper executions, a persistent inspector for whichever
 * node is selected, and a labeled Stop-all header control (tasks.md 4.2) --
 * the ONLY stop control in the composer/graph surface besides the
 * inspector's own per-node Stop agent (tasks.md 4.1).
 *
 * Every row is a direct projection of `session.executions`
 * (`ExecutionRecord`, `src/lib/bindings.ts`) via `buildGraphTree`
 * (`src/lib/agentGraphProjection.ts`) -- there is no separate frontend graph
 * state to go stale (tasks.md 6.1).
 */
export function AgentGraphPanel({ session }: { session: AgentSession }) {
  const qc = useQueryClient()
  const [stopping, setStopping] = useState(false)
  // Selection lives in the store so a "View in graph" message link can pick
  // a node from the conversation, even before this panel is open.
  const sessionId = session.header.sessionId
  const selectedId = useAgentDeskUiStore((s) => s.graphSelection[sessionId] ?? null)
  const selectGraphNode = useAgentDeskUiStore((s) => s.selectGraphNode)
  const setSelectedId = (id: string | null) => selectGraphNode(sessionId, id)

  const executions = session.executions
  const tree = buildGraphTree(executions)
  // Result records live in a sidecar the backend fills as each execution
  // ends (`build_result_for_completed_execution`); same query key the
  // conversation's review panel uses, so Keep/Commit there refreshes here.
  const resultsQuery = useQuery({
    queryKey: keys.agentResults(session.header.sessionId),
    queryFn: async () => unwrap(await commands.agentResultList(session.header.sessionId)),
    enabled: executions.length > 0,
  })
  const records = resultsQuery.data?.kind === 'found' ? resultsQuery.data.records : undefined
  const activeCount = executions.filter(
    (e) => runIsActive(e.state)
  ).length

  useEffect(() => {
    // Keep a selection alive across re-renders: default to the lead, and
    // fall back to the lead again if the selected node disappears (e.g. a
    // stopped helper eventually pruned from a later session read).
    if (selectedId && executions.some((e) => e.executionId === selectedId)) return
    const lead = executions.find((e) => e.parentExecutionId === null)
    setSelectedId(lead?.executionId ?? executions[0]?.executionId ?? null)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [session.header.sessionId, executions.length])

  const stopAll = async () => {
    if (stopping) return
    setStopping(true)
    try {
      const outcome = unwrap(await commands.agentSessionStopExecution(session.header.sessionId, { kind: 'all' }))
      if (outcome.kind === 'stopped') {
        void qc.invalidateQueries({ queryKey: keys.agentSession(session.header.sessionId) })
        toast.success(explainStopOutcome(outcome).message)
      } else if (outcome.kind === 'notFound') {
        toast.error('This chat is gone. It may have been archived elsewhere.')
      } else if (outcome.kind === 'damaged') {
        toast.error('This chat file is damaged and could not be stopped.', { description: outcome.reason })
      } else {
        toast.error(explainStopOutcome(outcome).message)
      }
    } catch (e) {
      const message = describeError(e)
      log.error(`agent desk: could not stop all executions: ${message}`)
      toast.error('Could not stop the agents.', { description: message })
    } finally {
      setStopping(false)
    }
  }

  if (executions.length === 0) {
    // tasks.md 7.5: explain Solo/Plan/Auto in plain language rather than
    // Deliberately NOT the mockup's copy, which names a lead agent "Sol" and
    // says "composer"/"graph". Nothing produces the name Sol -- see the same
    // refusal in SessionComposer -- and the other two are our own words for
    // parts the user never sees labelled that way.
    return (
      <div className="flex h-full flex-col items-center justify-center gap-2 px-4 py-8 text-center">
        <GitFork size={26} className="text-muted-foreground" aria-hidden />
        <p className="text-xs font-semibold text-foreground">Only one agent is on this chat.</p>
        <p className="max-w-[16rem] text-2xs leading-relaxed text-muted-foreground">
          Choose "A lead agent, up to 3 helpers" below to let one agent split the work. In Plan you approve the split
          first. In Auto the lead starts helpers on its own when the work divides safely.
        </p>
      </div>
    )
  }

  const selected = tree.find((n) => n.execution.executionId === selectedId) ?? tree[0]
  const awaitingStartLead = executions.find((e) => e.parentExecutionId === null && e.proposedGraph !== null)

  if (awaitingStartLead) {
    // tasks.md 2.2: Plan mode waits here until Start/Revise/Use-solo -- no
    // tree to show yet, since nothing has an execution beyond the lead
    // itself.
    return (
      <div className="flex h-full flex-col gap-2">
        <AwaitingStartCard
          session={session}
          lead={awaitingStartLead}
          onRevise={() => {
            // A button that only tells you to go do the thing elsewhere is
            // homework, not an action. Put the caret where the revision is
            // actually written; the toast then explains what to do there.
            //
            // It also seeds the box with the plan as it stands. "Say what to
            // change" over an empty box asks someone to describe from memory a
            // plan they can no longer see once they start typing -- the whole
            // reason revising in prose feels harder than it is. The seed is a
            // starting point to edit, not a message to send.
            const { getDraft, setDraft } = useAgentDeskUiStore.getState()
            const sessionId = session.header.sessionId
            // Never clobber something already typed: a half-written revision is
            // worth more than the seed.
            if (getDraft(sessionId).trim() === '') {
              setDraft(sessionId, revisionSeed(awaitingStartLead.proposedGraph!))
            }
            const box = document.getElementById(`agent-desk-composer-${sessionId}`)
            if (box instanceof HTMLTextAreaElement) {
              box.focus()
              box.setSelectionRange(box.value.length, box.value.length)
            }
            toast('Change the plan below, then send it back to the agent.')
          }}
        />
      </div>
    )
  }

  return (
    <div className="flex h-full flex-col gap-2">
      <div className="flex flex-none items-center gap-2 pb-1">
        <span className="min-w-0 flex-1 truncate text-2xs text-muted-foreground">{graphSummary(executions)}</span>
        <button
          type="button"
          onClick={() => void stopAll()}
          disabled={stopping || activeCount === 0}
          className="flex-none rounded border border-border bg-panel px-1.5 py-1 text-2xs font-semibold text-foreground hover:bg-panel3 disabled:cursor-not-allowed disabled:opacity-50"
        >
          {stopping ? 'Stopping…' : 'Stop all'}
        </button>
      </div>

      <div className="flex min-h-0 flex-1 flex-col gap-1.5 overflow-y-auto">
        {tree.map((node) => (
          <GraphNodeRow
            key={node.execution.executionId}
            node={node}
            activity={latestActivityLine(session.messages, node.execution.executionId)}
            selected={node.execution.executionId === selected?.execution.executionId}
            onSelect={() => setSelectedId(node.execution.executionId)}
          />
        ))}
      </div>

      {selected ? (
        <InspectorCard
          key={selected.execution.executionId}
          session={session}
          node={selected}
          activity={latestActivityLine(session.messages, selected.execution.executionId)}
          record={resultForNode(records, selected.execution.executionId)}
          onStopped={() => void qc.invalidateQueries({ queryKey: keys.agentSession(session.header.sessionId) })}
        />
      ) : null}
    </div>
  )
}
