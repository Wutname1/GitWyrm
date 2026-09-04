import type { StartExecutionOutcome, StartGraphOutcome } from '@/lib/bindings'

/**
 * What a person can do from a start-failure card. Each maps to one button;
 * the card component decides the label and the owner decides the handler.
 */
export type StartFailureAction = 'openProject' | 'pickProvider' | 'tryAgain'

/**
 * A start failure the UI should keep on screen until the person acts on it
 * (source-kickoffs tasks 2.4 and 5.3). A toast vanishes in seconds, and
 * someone who stepped away comes back to a saved message with no hint why
 * nothing happened; this card is what stays behind.
 */
export type StartFailureCard = {
  /** The outcome `kind` this card came from, so tests and keys can name it. */
  kind: string
  /** One short line saying what went wrong, in everyday words. */
  title: string
  /** One or two short sentences saying what to do about it. */
  body: string
  /** The backend's own wording, shown smaller under the body when present. */
  detail: string | null
  /** Buttons in display order. Empty means only Close is offered. */
  actions: StartFailureAction[]
}

/** Plain-language reason a proposed graph failed to validate, matching
 * `GraphValidationError`'s cases without exposing internal field names. */
export function invalidGraphReason(reason: string): string {
  switch (reason) {
    case 'tooManyHelpers':
      return 'This plan has more than 3 helpers, which is not allowed yet.'
    case 'duplicateNodeId':
      return 'Two helpers in this plan have the same id.'
    case 'unknownDependency':
      return "One helper depends on a step that isn't in this plan."
    case 'cycle':
      return 'Two or more helpers depend on each other in a loop.'
    case 'emptyJob':
      return 'One helper is missing a title or a real time budget.'
    case 'missingAllowedPaths':
      return "One helper can write files but wasn't told which ones it may touch."
    default:
      return 'This plan is no longer valid.'
  }
}

const SOURCE_MISSING: Omit<StartFailureCard, 'detail'> = {
  kind: 'sourceMissing',
  title: 'This chat needs its project open',
  body: 'The project this chat works on is not open right now. Open it, then try again.',
  actions: ['openProject', 'tryAgain'],
}

/**
 * Maps a message-send start outcome to the card the composer should show,
 * or `null` when nothing should stay on screen:
 *
 * - `started` succeeded, so any older card is cleared by the caller.
 * - `alreadyRunning` is not a failure. The message is saved and the next
 *   turn picks it up on its own, so it keeps its toast.
 * - `notFound` and `damaged` have nothing a person can do from the
 *   composer (the chat itself is gone or unreadable), so they stay toasts
 *   rather than offering a Try again that can only fail the same way.
 */
export function startFailureCardForExecution(outcome: StartExecutionOutcome): StartFailureCard | null {
  switch (outcome.kind) {
    case 'started':
    case 'alreadyRunning':
    case 'notFound':
    case 'damaged':
      return null
    case 'sourceMissing':
      return { ...SOURCE_MISSING, detail: outcome.detail }
    case 'adapterUnsupported':
      // Covers both "the tool is missing or too old" and "this tool cannot
      // do this kind of job" (the backend folds the capability refusal into
      // the same variant), so the fix is the same either way: another tool.
      return {
        kind: outcome.kind,
        title: 'This AI tool cannot run this chat right now',
        body: 'It may be missing, out of date, or unable to do this kind of work. Pick another AI tool, or try again.',
        detail: outcome.detail,
        actions: ['pickProvider', 'tryAgain'],
      }
    case 'providerReconnect':
      return {
        kind: outcome.kind,
        title: 'Your AI tool needs you to sign in again',
        body: 'Sign back in to that tool, then try again. Or pick a different AI tool.',
        detail: outcome.detail,
        actions: ['pickProvider', 'tryAgain'],
      }
    case 'unsupportedProvider':
      return {
        kind: outcome.kind,
        title: `This app cannot use "${outcome.requested}"`,
        body: 'That AI tool is not one this version of the app knows how to run. Pick another one, then try again.',
        detail: null,
        actions: ['pickProvider', 'tryAgain'],
      }
    case 'worktreeFailed':
      return {
        kind: outcome.kind,
        title: 'Could not set up a separate workspace',
        body: 'This kind of job runs in its own copy of the project, and that copy could not be made. Your files were not touched. Try again.',
        detail: outcome.detail,
        actions: ['tryAgain'],
      }
    case 'writeFailed':
      return {
        kind: outcome.kind,
        title: 'Could not save the start of this run',
        body: 'Your message is saved, but the run could not be recorded. Try again.',
        detail: outcome.detail,
        actions: ['tryAgain'],
      }
    case 'unavailable':
      return {
        kind: outcome.kind,
        title: 'The agent could not start',
        body: 'Something on this computer got in the way. Your message is saved. Try again.',
        detail: outcome.detail,
        actions: ['tryAgain'],
      }
    default:
      return unknownFailure(outcome)
  }
}

/**
 * Maps a Plan-mode Start outcome (`AwaitingStartCard`) to a card, or `null`:
 *
 * - `stale` has its own banner with Start anyway / Revise, so it is not a
 *   card here.
 * - `alreadyStarted` means a racing click already won; the caller refreshes
 *   and the plan card goes away on its own.
 * - `noProposal`, `notFound`, `damaged` have nothing Try again can fix.
 */
export function startFailureCardForGraph(outcome: StartGraphOutcome): StartFailureCard | null {
  switch (outcome.kind) {
    case 'started':
    case 'stale':
    case 'alreadyStarted':
    case 'noProposal':
    case 'notFound':
    case 'damaged':
    // The lead said it will work alone. Nothing failed, so no card.
    case 'noHelpersRunSolo':
      return null
    case 'sourceMissing':
      return { ...SOURCE_MISSING, detail: outcome.detail }
    case 'invalid':
      return {
        kind: outcome.kind,
        title: 'This plan can no longer start',
        body: 'Revise the plan to fix it, or try again if it was just changed.',
        detail: invalidGraphReason(outcome.reason.kind),
        actions: ['tryAgain'],
      }
    case 'worktreeFailed':
      return {
        kind: outcome.kind,
        title: `Could not set up a workspace for "${outcome.node_id}"`,
        body: 'Each helper works in its own copy of the project, and that copy could not be made. No helper was started. Try again.',
        detail: outcome.detail,
        actions: ['tryAgain'],
      }
    case 'writeFailed':
      return {
        kind: outcome.kind,
        title: 'Could not save the start of this plan',
        body: 'Nothing was started. Try again.',
        detail: outcome.detail,
        actions: ['tryAgain'],
      }
    case 'unavailable':
      return {
        kind: outcome.kind,
        title: 'The plan could not start',
        body: 'Something on this computer got in the way. Nothing was started. Try again.',
        detail: outcome.detail,
        actions: ['tryAgain'],
      }
    default:
      return unknownFailure(outcome)
  }
}

/**
 * A thrown error (IPC failure, unexpected exception) rather than a typed
 * outcome. Still worth a card: the person has a saved message and no run.
 */
export function startFailureCardForError(message: string): StartFailureCard {
  return {
    kind: 'error',
    title: 'The agent could not start',
    body: 'Your message is saved. Try again.',
    detail: message,
    actions: ['tryAgain'],
  }
}

/** A `kind` this build of the UI does not know. Shows what it can and offers a retry. */
function unknownFailure(outcome: { kind: string }): StartFailureCard {
  const detail = 'detail' in outcome && typeof outcome.detail === 'string' ? outcome.detail : null
  return {
    kind: outcome.kind,
    title: 'The agent could not start',
    // Agent Desk is its own window and has no Settings in it, so "send a
      // bug report from Settings" named a place with no path from here. The
      // graph panel already says "Open the main GitWyrm window first" for
      // the same reason; this string was left behind.
      body: 'Try again. If it keeps happening, open the main GitWyrm window and send a bug report from Settings.',
    detail: detail ?? outcome.kind,
    actions: ['tryAgain'],
  }
}
