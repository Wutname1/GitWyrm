import type {
  IssueDetail,
  IssueSummary,
  PrDetail,
  PrSummary,
  ProviderId,
  SessionSourceInput,
  SpecChange,
  RefreshSourceOutcome,
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

/**
 * Plain-language line saying how old the saved copy of a source is, and
 * whether it is still the live one.
 *
 * A session keeps a snapshot of what started it precisely so the history
 * still reads correctly after the issue, pull request or spec moves on. That
 * only works if the person can tell they are looking at a saved copy: without
 * it, a months-old issue summary reads as current, and "No longer available"
 * says the source is gone without saying what the panel is still showing.
 *
 * Returns `null` when there is nothing honest to say -- a manual session has
 * no source, and an unparseable timestamp is not worth guessing at.
 */
export function describeSnapshotFreshness(
  capturedAt: string,
  liveUnavailable: boolean,
  now: number = Date.now()
): string | null {
  const then = Date.parse(capturedAt)
  if (Number.isNaN(then)) return null

  const mins = Math.max(0, Math.floor((now - then) / 60000))
  let age: string
  if (mins < 1) age = 'just now'
  else if (mins < 60) age = `${mins} minute${mins === 1 ? '' : 's'} ago`
  else {
    const hours = Math.floor(mins / 60)
    if (hours < 24) age = `${hours} hour${hours === 1 ? '' : 's'} ago`
    else {
      const days = Math.floor(hours / 24)
      age = days < 30 ? `${days} day${days === 1 ? '' : 's'} ago` : 'a long time ago'
    }
  }

  // When the live source cannot be reached, the saved copy is all there is --
  // say so, rather than leaving "No longer available" to imply the panel is
  // showing nothing.
  return liveUnavailable ? `Saved copy from ${age}. This is what the chat still shows.` : `Checked ${age}.`
}

/**
 * Plain-language result of refreshing a session's source, and whether the
 * refresh actually reached the live source.
 *
 * The drift banner used to tell people to "Refresh the source" while the
 * command behind it had no button anywhere in the app -- an instruction
 * without an action, which reads to a beginner as their own mistake. Now the
 * button exists, this says what it did.
 */
export function explainRefreshSourceOutcome(outcome: RefreshSourceOutcome): { message: string; ok: boolean } {
  switch (outcome.kind) {
    case 'refreshed':
      return outcome.changed
        ? { message: 'Refreshed. The chat now shows the current version.', ok: true }
        : { message: 'Checked -- the source had not changed after all.', ok: true }
    case 'liveUnavailable':
      return {
        message: `Could not reach the original, so the saved copy was kept: ${outcome.detail}`,
        ok: false,
      }
    case 'notFound':
      return { message: 'That chat could not be found.', ok: false }
    case 'damaged':
      return { message: `That chat's file is damaged: ${outcome.reason}`, ok: false }
    case 'unavailable':
      return { message: `That chat could not be read right now: ${outcome.detail}`, ok: false }
    case 'writeFailed':
      return { message: `Could not save the refreshed copy: ${outcome.detail}`, ok: false }
  }
}
