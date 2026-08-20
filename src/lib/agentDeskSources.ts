import type {
  IssueDetail,
  IssueSummary,
  PrDetail,
  PrSummary,
  ProviderId,
  SessionSourceInput,
  SpecChange,
  SpecTask,
} from '@/lib/bindings'

/**
 * Builds an Agent Desk `SessionSourceInput` for an issue from whatever is
 * already loaded -- either the dense row (`IssueSummary`, no body) or the
 * full panel (`IssueDetail`, with body/comments). Task 3.3: "Build the
 * launch snapshot from loaded issue number/title/body/labels/assignee/URL."
 *
 * Deliberately accepts either shape rather than requiring the detail fetch
 * first -- architecture.md section 8: "Do not fetch all source details in
 * the main window before opening Agent Desk. Create from known row data,
 * then enrich in the Desk." A context-menu click from the sidebar (which
 * only has the summary row) must not block on a network round-trip just to
 * build this input.
 *
 * Returns a `SessionSourceInput`, not a `SessionSource` -- the backend
 * (`commands::agent_kickoff::agent_session_start`) stamps the source
 * snapshot's `capturedAt`/`liveUnavailable` itself, so the frontend never
 * constructs a `SourceSnapshot` claiming a capture time or live-availability
 * state it does not actually know.
 */
export function issueSourceInput(
  hostId: ProviderId,
  owner: string,
  repo: string,
  issue: IssueSummary | IssueDetail
): Extract<SessionSourceInput, { kind: 'issue' }> {
  const body = 'body' in issue ? issue.body : ''
  const labels = issue.labels.length > 0 ? ` [${issue.labels.join(', ')}]` : ''
  const assignee = issue.assignee ? ` — assigned to ${issue.assignee}` : ' — unassigned'
  return {
    kind: 'issue',
    hostId,
    owner,
    repo,
    number: issue.number,
    url: issue.html_url,
    title: issue.title,
    summary: `${body}${labels}${assignee}`.trim(),
  }
}

/**
 * Builds an Agent Desk `SessionSourceInput` for a pull request from whatever
 * is already loaded -- the dense row (`PrSummary`) or the full panel
 * (`PrDetail`). Task 4.3: "Snapshot PR metadata, head/base, draft/state,
 * author, URL, and known checks."
 */
export function pullRequestSourceInput(
  hostId: ProviderId,
  owner: string,
  repo: string,
  pr: PrSummary | PrDetail
): Extract<SessionSourceInput, { kind: 'pullRequest' }> {
  const body = 'body' in pr ? pr.body : ''
  const state = 'merged' in pr && pr.merged ? 'merged' : pr.draft ? 'draft' : 'open'
  const summaryParts = [body, `${pr.head_ref} -> ${pr.base_ref}`, `by ${pr.author}`, state].filter(
    Boolean
  )
  return {
    kind: 'pullRequest',
    hostId,
    owner,
    repo,
    number: pr.number,
    url: pr.html_url,
    head: pr.head_ref,
    base: pr.base_ref,
    title: pr.title,
    summary: summaryParts.join(' — '),
  }
}

/**
 * Builds an Agent Desk `SessionSourceInput` for a whole OpenSpec change --
 * package `agent-desk-openspec-workflows` tasks.md 1.1/1.3. `change` is
 * whatever `useOpenspecChanges`/`useSelectedChange` already has loaded (a
 * full `SpecChange`, same as `DeskDetail` renders), so this never fetches
 * anything new before Agent Desk opens.
 */
export function openSpecChangeSourceInput(
  change: SpecChange
): Extract<SessionSourceInput, { kind: 'openSpecChange' }> {
  return {
    kind: 'openSpecChange',
    changeId: change.id,
    title: change.title,
    summary: change.proposal.why || change.proposal.raw,
  }
}

/**
 * Builds an Agent Desk `SessionSourceInput` for one exact task inside an
 * OpenSpec change -- the "specific task" entry point Gate 4 exercises.
 * `task.index`/`task.text` are the identity a session keeps naming even when
 * the task is not the next open one (tasks.md 1.2, spec scenario "Non-next
 * task"), so this must be called with the parsed task the user actually
 * clicked, not a recomputed "next open task."
 */
export function openSpecTaskSourceInput(
  change: SpecChange,
  task: SpecTask
): Extract<SessionSourceInput, { kind: 'openSpecTask' }> {
  return {
    kind: 'openSpecTask',
    changeId: change.id,
    taskIndex: task.index,
    taskText: task.text,
    title: change.title,
    summary: task.text,
  }
}
