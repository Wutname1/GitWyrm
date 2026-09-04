import type { AgentSession, HelperRole, ResultRecord, SessionMessage } from '@/lib/bindings'

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

/**
 * What a helper's job is, in words a person uses.
 *
 * `HelperRole` is stored as `researcher`/`builder`/`verifier`, and both places
 * that showed it printed the stored token verbatim -- including the approval
 * card, the one screen where someone authorises agents to change their files.
 * House rule: no internal token reaches the user.
 *
 * An unrecognised value falls back to "Helper" rather than leaking the raw
 * string, matching how `changedPathStatusLabel` handles an unknown status code.
 */
export function helperRoleLabel(role: HelperRole | string | null | undefined): string {
  switch (role) {
    case 'researcher':
      return 'Looks things up'
    case 'builder':
      return 'Makes the changes'
    case 'verifier':
      return 'Checks the work'
    default:
      return 'Helper'
  }
}

/**
 * Which files a helper is allowed to change, said plainly.
 *
 * `allowedPaths` is the boundary the backend actually enforces
 * (`policy.rs`'s `check_path_allowance` refuses a write outside it), and it
 * rendered in no component at all -- so someone pressing Start authorised
 * file-writing agents while the scope of that permission was the one fact
 * they could not see.
 *
 * An empty list means the policy is not path-scoped, which is *wider*, not
 * narrower -- so it says so rather than showing nothing and reading as
 * "no files".
 */
export function allowedPathsLabel(paths: string[]): string {
  if (paths.length === 0) return 'Any file in this project'
  if (paths.length <= 3) return paths.join(', ')
  return `${paths.slice(0, 3).join(', ')} and ${paths.length - 3} more`
}

/**
 * The allowed paths as separate lines, plus how many are not shown.
 *
 * `allowedPathsLabel` joins them into one string, which the approval card then
 * truncated to a single line: three realistic paths are ~155 characters, and
 * the panel can be as narrow as `MIN_CHAT_SIZE_PX` (360px) at `text-2xs`, so
 * a person saw the first path and an ellipsis. These paths are the permission
 * being granted, so they get a line each and the overflow is counted rather
 * than cut mid-path.
 *
 * `rest` is 0 when everything fits. An empty list yields no lines at all --
 * the caller says "Any file in this project", which is wider, not narrower.
 */
export function allowedPathLines(paths: string[], limit = 3): { lines: string[]; rest: number } {
  if (paths.length === 0) return { lines: [], rest: 0 }
  return { lines: paths.slice(0, limit), rest: Math.max(0, paths.length - limit) }
}
