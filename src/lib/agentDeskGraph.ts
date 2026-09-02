import type { AgentSession } from '@/lib/bindings'

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
