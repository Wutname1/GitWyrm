import type { GateRequest, RunStep, SessionMessage } from '@/lib/bindings'

/**
 * Pure display logic for an approval gate (`RunStep::Gate`), kept out of
 * `ConversationPane` so it is testable under `vitest`'s Node environment --
 * the same pattern `agentDeskResult.ts`/`agentDeskPlan.ts` use.
 *
 * Gap 4 of the 2026-08-21 implementation reset: `agent_session_answer_gate`
 * (backend, `commands::agent_desk`) has existed with no caller anywhere in
 * the frontend. A `RunStep::Gate` message reaches the transcript already
 * (`agentdesk::bridge::message_kind_for_step` maps it to
 * `MessageKind::Approval`, and `ConversationPane` already styles that kind
 * amber), but nothing ever answered it -- an Auto run that hit a
 * destructive-action approval just sat there with no visible way forward.
 * This module turns the message's own `renderedContent` (the full `RunStep`,
 * serialized losslessly -- see `agentdesk::bridge::map_run_step`'s doc
 * comment) back into a `GateRequest` and a plain-language title/option pair,
 * so the transcript row can render real buttons instead of dead amber text.
 */

/** The `RunStep::Gate` this message carries, or `null` if it is not a gate/cannot be parsed. */
export function gateRequestOf(message: SessionMessage): GateRequest | null {
  if (message.kind !== 'approval' || !message.renderedContent) return null
  try {
    const step = JSON.parse(message.renderedContent) as RunStep
    if (step.kind === 'gate') return step.request
    return null
  } catch {
    return null
  }
}

/** Plain-language, request-specific summary of what is being approved. */
export function gateSummary(request: GateRequest): string {
  switch (request.kind) {
    case 'addDependency':
      return `Add the "${request.name}" package to this project?`
    case 'runInstall':
      return `Run "${request.command}" to install something?`
    case 'networkAccess':
      return `Let it reach ${request.target} over the network?`
    case 'deleteFiles':
      return request.paths.length === 1
        ? `Delete ${request.paths[0]}?`
        : `Delete ${request.paths.length} files, including ${request.paths[0]}?`
    case 'outsideRepo':
      return `Change something outside this project, at ${request.path}?`
    case 'publish':
      // Named as leaving the machine, because that is the consequence the
      // person is actually approving. A publishing command reaches the gate
      // looking like any other shell write, so this wording is the only
      // thing that distinguishes it.
      return `Send work out of this project -- ${request.effect}?`
    case 'unclassified':
      return request.summary
  }
}

/** One answer button: its label and the `GateAnswer` it sends. */
export interface GateOption {
  label: string
  answer: 'allowOnce' | 'findAnotherWay' | 'stopRun'
}

/**
 * The three answers every gate offers, in the order they should be shown.
 * Always the same three -- `GateAnswer` has no request-specific variants --
 * but `gateSummary` above is what keeps the wording specific to what is
 * being approved, not a generic "Allow/Deny", per Rule #2 (plain language,
 * consequence-focused).
 */
export function gateOptions(): GateOption[] {
  return [
    { label: 'Allow it', answer: 'allowOnce' },
    { label: 'Find another way', answer: 'findAnotherWay' },
    { label: 'Stop the run', answer: 'stopRun' },
  ]
}

/**
 * The consequence paragraph for a gate, re-exported so the transcript's
 * inline gate and the standalone card cannot drift apart.
 *
 * It lives in `GateCard` because that is where it was written; what matters
 * is that there is exactly one of it. The transcript used to render only
 * `gateSummary`, so the warnings that exist because undo cannot help --
 * a command reaching outside the project, or one sending work off this
 * machine -- never reached the person answering the gate, which is the
 * only place they matter.
 */
/**
 * What actually happens if this is allowed.
 *
 * This lived in `ai-run/GateCard`, a component nothing mounts, and the live
 * transcript gate imported it back out -- so the sentences that exist because
 * undo cannot help (reaching outside the project, sending work off this
 * machine) were sourced from dead code. It belongs here, beside `gateSummary`,
 * with the rest of the gate's display logic.
 */
export function gateBody(request: GateRequest): string {
  switch (request.kind) {
    case 'addDependency':
      return `This downloads ${request.name} from the internet and adds it to your project's list of libraries. You'll see the change before anything is committed.`
    case 'runInstall':
      return `This runs ${request.command}, which downloads code from the internet onto this machine.`
    case 'networkAccess':
      return `This sends a request to ${request.target} and waits for a reply.`
    case 'deleteFiles':
      return `This removes ${request.paths.join(', ')} from your folder. You can undo the whole run afterwards.`
    case 'outsideRepo':
      // The one gate whose consequence outlives the run, so it says so.
      return `${request.path} is outside the folder you opened, so it isn't covered by undo. Changes there stay even if you undo this run.`
    case 'publish':
      // The one gate whose consequence leaves this machine, so it says so
      // plainly: undo can put your own files back, but it cannot recall
      // something other people can already see.
      return `This would ${request.effect}, so it leaves this computer and other people may see it. Undoing the run afterwards cannot take it back.`
    case 'unclassified':
      // Shown when GitWyrm cannot tell what the AI is asking for. Says exactly
      // that rather than dressing it up as a known kind of request, which
      // would tell the user the wrong thing about what they are approving.
      return `The AI asked to do something GitWyrm doesn't recognise: ${request.summary}. Only allow it if you understand what it will do.`
  }
}
