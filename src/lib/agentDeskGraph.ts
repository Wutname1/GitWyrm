import type { AgentSession, ResultRecord, SessionMessage } from '@/lib/bindings'

/**
 * Whether this chat has an agent graph worth showing.
 *
 * Most chats are one agent talking to one person. Offering a Graph panel on
 * every one of them made the feature look broken: open it and there is
 * nothing there. So the panel is offered only once a graph exists in some
 * form: a helper has been created, a lead has proposed a team and is waiting
 * for Start, or the graph has been started. Any one of those is enough; the
 * panel itself explains the rest.
 */
export function sessionHasGraph(session: Pick<AgentSession, 'header' | 'executions'> | null | undefined): boolean {
  if (!session) return false
  if (session.header.graphStartedAt) return true
  return session.executions.some((e) => e.parentExecutionId !== null || e.proposedGraph !== null)
}

type ActivityMessage = Pick<SessionMessage, 'executionId' | 'role' | 'kind' | 'sequence' | 'plainContent'>

/**
 * The newest thing one execution said about its own work, as a single line
 * for the graph node card (tasks.md 6.2 "show its current action").
 *
 * Mirrors `agentdesk::graph::latest_activity_for` on the backend so the
 * Graph panel, which already holds the whole `AgentSession`, can show it
 * without another round trip. Tool activity, notes, thought summaries,
 * approvals and system lines all count; what the person typed and the
 * final `result` do not (the result is the node's output, shown
 * separately). Newest is decided by `sequence`, with transcript order as
 * the tie-break for messages that carry none, so a late-arriving earlier
 * event cannot overwrite what the node is doing now.
 */
export function latestActivityLine(messages: ActivityMessage[], executionId: string): string | null {
  let best: { sequence: number; text: string } | null = null
  for (const m of messages) {
    if (m.executionId !== executionId) continue
    if (m.role === 'user' || m.kind === 'result') continue
    // Later array position wins a tie because `>=` replaces on equal
    // sequence; a message with no sequence sorts below every numbered one.
    const sequence = m.sequence ?? -1
    if (best === null || sequence >= best.sequence) {
      best = { sequence, text: m.plainContent }
    }
  }
  if (best === null) return null
  const line = best.text
    .split('\n')
    .map((l) => l.trim())
    .find((l) => l.length > 0)
  return line ?? null
}

/** The result record captured for one execution, if the backend has built
 * one yet. Absent (not "empty") until then, so callers omit rather than
 * disable the View changes / output controls. */
export function resultForNode(records: ResultRecord[] | undefined, executionId: string): ResultRecord | null {
  return records?.find((r) => r.executionId === executionId) ?? null
}

/** Whether "View changes" can open something: a result with a worktree on
 * disk and at least one changed file. A read-only helper never has a
 * worktree, and a builder that changed nothing has nothing to diff. */
export function canViewNodeChanges(record: Pick<ResultRecord, 'worktreePath' | 'changedPaths'> | null): boolean {
  return record !== null && record.worktreePath !== null && record.changedPaths.length > 0
}
