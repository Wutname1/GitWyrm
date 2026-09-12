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
  const assignee = issue.assignee ? `, assigned to ${issue.assignee}` : ', unassigned'
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
    summary: summaryParts.join(' · '),
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
/**
 * How long ago something was, in plain words.
 *
 * Lifted out of `describeSnapshotFreshness` so the import badge and the
 * config receipt list can say when something happened using exactly the same
 * vocabulary -- several places describing age differently is the kind of small
 * inconsistency that makes a surface feel assembled rather than designed.
 */
export function describeAge(elapsedMs: number): string {
  const mins = Math.max(0, Math.floor(elapsedMs / 60000))
  if (mins < 1) return 'just now'
  if (mins < 60) return `${mins} minute${mins === 1 ? '' : 's'} ago`
  const hours = Math.floor(mins / 60)
  if (hours < 24) return `${hours} hour${hours === 1 ? '' : 's'} ago`
  const days = Math.floor(hours / 24)
  return days < 30 ? `${days} day${days === 1 ? '' : 's'} ago` : 'a long time ago'
}

export function describeSnapshotFreshness(
  capturedAt: string,
  liveUnavailable: boolean,
  now: number = Date.now()
): string | null {
  const then = Date.parse(capturedAt)
  if (Number.isNaN(then)) return null

  const age = describeAge(now - then)

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
        : { message: 'Checked: the source had not changed after all.', ok: true }
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

/**
 * A chat started from one commit.
 *
 * The `SessionSource::Commit` variant has been typed, persisted, converted by
 * the backend and rendered by the UI since sources shipped -- with **no
 * builder**, so nothing in the app could ever create one. The vision names
 * commits, diffs and failed checks among the things a chat can start from;
 * without these functions, that sentence described a capability the type
 * system supported and no gesture could reach.
 *
 * The summary is what the chat shows when the commit is no longer reachable,
 * so it carries the message rather than only the id.
 */
export function commitSourceInput(
  oid: string,
  subject: string,
  author: string,
  when: string
): Extract<SessionSourceInput, { kind: 'commit' }> {
  const shortOid = oid.slice(0, 7)
  return {
    kind: 'commit',
    oid,
    title: subject || `Commit ${shortOid}`,
    summary: [subject, author ? `by ${author}` : '', when].filter(Boolean).join(' · '),
  }
}

/**
 * **No caller yet.** The backend accepts this source kind in full -- durable
 * identity, a cached snapshot, a stable key -- and nothing in the app offers
 * a way to start a chat from a diff.
 *
 * Worth naming rather than leaving to be rediscovered. The product describes
 * what starts a chat as "an issue, a pull request, a diff, a failed check, or
 * an OpenSpec task". Issue, pull request, commit, both OpenSpec kinds and the
 * uncommitted changes are reachable from a screen. This one is built, stored
 * and tested with no menu item anywhere. A failed check is further behind
 * still: the backend has the shape, and there is no builder here at all.
 *
 * Kept because the plumbing is right and only the entry point is absent, so
 * the day a diff view grows a "Fix with AI" action this is what it calls.
 *
 * **Deliberately not hung off the commit menu**, which was argued out on
 * 2026-09-12. A commit already has its own source kind and its own entry
 * point, and a chat started from one runs read-only against the real
 * repository holding the real sha -- so `git show` yields every path and
 * every hunk, which is strictly more than this builder's path list. Wiring it
 * there would put a second AI verb on the app's longest context menu with no
 * difference a person could perceive, and would store `diff:<sha>:<paths>`
 * beside `commit:<sha>` as two identities for one real-world thing.
 *
 * `scope` is a SELECTOR -- "staged", "unstaged", a glob, a ref range -- which
 * is what the tests use it for. A commit is an object, not a selector, and it
 * already has a variant. The right home is a surface where a set of changed
 * files is the unit and no commit owns it.
 *
 * A chat started from a set of changed files -- a commit's diff, or the
 * staged/unstaged view.
 *
 * `scope` is the backend's own word for which diff this was, kept verbatim so
 * a stored source still says what it pointed at after the working tree moves.
 */
export function diffSourceInput(
  scope: string,
  paths: string[],
  label: string
): Extract<SessionSourceInput, { kind: 'diff' }> {
  const count = paths.length
  return {
    kind: 'diff',
    scope,
    paths,
    title: label,
    summary: count === 1 ? '1 changed file' : `${count} changed files`,
  }
}

/** A chat started from whatever is uncommitted in the project right now. */
export function workingChangesSourceInput(paths: string[]): Extract<SessionSourceInput, { kind: 'workingChanges' }> {
  const count = paths.length
  return {
    kind: 'workingChanges',
    paths,
    title: count === 1 ? 'Your 1 changed file' : `Your ${count} changed files`,
    summary: count === 0 ? 'Nothing is changed right now' : paths.slice(0, 5).join(', ') + (count > 5 ? `, and ${count - 5} more` : ''),
  }
}

/**
 * When an imported message actually arrived in GitWyrm.
 *
 * `ImportProvenance` carries `importedAt` specifically because it is "distinct
 * from the message's own timestamp" -- a conversation written last Tuesday and
 * pulled in today is two different facts. The badge showed only which client
 * it came from, so an imported message sat in the transcript looking like
 * native history dated whenever it was originally written. The vision is
 * explicit that import is never presented as equivalent to work done here.
 *
 * Returns null for an unparseable timestamp rather than inventing a date --
 * the badge still says "Imported", it just does not claim a time it does not
 * know.
 */
export function describeImportedAt(importedAt: string, now: number = Date.now()): string | null {
  const then = Date.parse(importedAt)
  if (Number.isNaN(then)) return null
  return `Brought into GitWyrm ${describeAge(now - then)}`
}

/**
 * A message's clock time, e.g. "2:05 PM", in the reader's own locale.
 *
 * Written out twice, byte for byte, in `ConversationPane` and
 * `MessageHistoryRail` -- two components that show the same timestamps beside
 * each other, so any drift between them would appear as the transcript and its
 * rail disagreeing about when something happened. Returns an empty string for
 * an unparseable timestamp rather than "Invalid Date".
 */
export function formatClock(iso: string): string {
  const t = Date.parse(iso)
  if (Number.isNaN(t)) return ''
  return new Date(t).toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' })
}
