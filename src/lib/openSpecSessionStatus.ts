import type { OpenSpecContextDriftOutcome, OpenSpecSessionStatus, OpenSpecSourceContext, OpenSpecSourceOutcome } from '@/lib/bindings'

export type OpenSpecStatusTone = 'neutral' | 'amber' | 'red'

export interface OpenSpecStatusLine {
  text: string
  tone: OpenSpecStatusTone
}

/**
 * Plain-language line for an OpenSpec session source's archived/moved/
 * deleted state (`agent-desk-openspec-workflows` tasks.md 4.5, section 7).
 *
 * `null` means "show nothing here" -- either everything is fine (`active`),
 * the question does not apply (`notAnOpenSpecSource`), or the status could
 * not be determined right now (session-level failures), in which case the
 * caller's own generic "no longer available" fallback already covers it.
 *
 * Pulled out of `SessionSourcePanel.tsx` so this mapping -- which state gets
 * which words and which severity -- is covered by a fast `.test.ts` unit
 * test; this project's `vitest.config.ts` runs `src/**\/*.test.ts` in a Node
 * environment with no DOM, so the component itself is not unit-testable
 * here (see `src/lib/agentDeskTargets.ts` for the same extraction pattern).
 */
export function openSpecStatusLine(status: OpenSpecSessionStatus | undefined): OpenSpecStatusLine | null {
  if (!status) return null
  switch (status.kind) {
    case 'active':
    case 'notAnOpenSpecSource':
      return null
    case 'archived':
      return { text: 'This change is finished and archived.', tone: 'neutral' }
    case 'moved':
      return {
        text: status.archived
          ? `Looks like it was renamed to "${status.likelyNewId}" and archived.`
          : `Looks like it was renamed to "${status.likelyNewId}".`,
        tone: 'amber',
      }
    case 'deleted':
      return { text: 'This change no longer exists in the repository.', tone: 'amber' }
    case 'repoNotOpen':
      return { text: 'Open this repository to see its current status.', tone: 'neutral' }
    case 'sessionNotFound':
    case 'sessionDamaged':
    case 'sessionUnavailable':
      return null
  }
}

/**
 * How much of an OpenSpec change is done, for the source panel.
 *
 * OpenSpec is the thing Agent Desk knows that a generic chat window does not,
 * and none of the panels said how far along the change was -- the context
 * command that answers it had no caller at all. This is the one line worth
 * putting on the source panel: a person looking at an OpenSpec session wants
 * to know how much is left before they read anything else.
 *
 * Every non-`found` outcome returns null rather than a zero. A change we
 * could not read is not a change with no progress, and the vision is explicit
 * that unknown stays unknown. `isDraft` is likewise not 0% -- the backend
 * distinguishes "no tasks yet" from "none of the tasks are done", and so does
 * this.
 */
export function openSpecProgressLine(
  outcome: OpenSpecSourceOutcome<OpenSpecSourceContext> | undefined
): string | null {
  if (!outcome || outcome.kind !== 'found') return null
  const { progress } = outcome.value
  if (progress.is_draft) return 'No tasks written yet'
  if (progress.total === 0) return null
  // A one-task change is the common case for a task-scoped session, so the
  // singular is not an edge case here.
  const tasks = (n: number) => `${n} task${n === 1 ? '' : 's'}`
  if (progress.done === progress.total) {
    return progress.total === 1 ? 'The one task is done' : `All ${tasks(progress.total)} done`
  }
  return `${progress.done} of ${tasks(progress.total)} done`
}

/**
 * Why the spec-drift check could not answer, when it could not.
 *
 * The panel tested `kind === 'checked' && diverged`, so five failure modes --
 * the repo not open, no spec folder, a damaged or unreadable session file --
 * rendered exactly like "the spec has not changed since the agent read it".
 * That is a blocking safety signal reporting a false negative: a silent
 * all-clear on a check that never ran.
 *
 * `checked` and `nothingToCompare` return null: the first is a real answer the
 * caller reads directly, and the second means there is genuinely nothing to
 * compare against, which is not a failure.
 */
export function explainDriftUnavailable(
  outcome: OpenSpecContextDriftOutcome | undefined,
  isError: boolean
): string | null {
  if (isError) return 'GitWyrm could not check whether the plan has changed since the agent read it.'
  if (!outcome) return null
  switch (outcome.kind) {
    case 'checked':
    case 'nothingToCompare':
      return null
    case 'repoNotOpen':
      return 'This project is not open, so GitWyrm cannot tell whether the plan has changed.'
    case 'noOpenSpecFolder':
      return 'This project has no plan folder any more, so there is nothing to compare against.'
    case 'sessionNotFound':
      return 'That chat is no longer there, so its plan cannot be compared.'
    case 'sessionDamaged':
      return `That chat's saved file could not be read, so the plan could not be compared: ${outcome.reason}`
    case 'sessionUnavailable':
      return `GitWyrm could not read that chat right now, so the plan could not be compared: ${outcome.detail}`
  }
}
