import { toast } from 'sonner'
import type { CliOutcome } from '@/lib/bindings'

/**
 * Strip ANSI colour codes and box-drawing leftovers from CLI output.
 *
 * The tool writes colour escapes even when its output is piped, which reach a
 * toast as literal `[31m` garbage next to the words.
 */
function clean(text: string): string {
  return text
    // eslint-disable-next-line no-control-regex
    .replace(/\[[0-9;]*m/g, '')
    .replace(/\[[0-9]{1,2}m/g, '')
    .replace(/[─-╿]/g, '')
    .trim()
}

/**
 * The one line worth showing from a failed CLI run.
 *
 * The tool prints a whole transcript -- task status, a plan of what it would
 * touch, running totals -- and then the actual problem. A toast that repeats all
 * of it is unreadable at a glance, which is the only thing a toast is for. The
 * inline check result panel is where the full output belongs.
 *
 * Picks the last line that reads like a reason, skipping the bookkeeping the
 * tool emits around it.
 */
export function cliReason(output: string): string | undefined {
  const lines = clean(output)
    .split('\n')
    .map((l) => l.trim())
    .filter(Boolean)
    // Progress and summary lines describe what the tool did, not what went
    // wrong. "Aborted" says only that it stopped, never why.
    .filter((l) => !/^(task status|specs to update|applying changes|totals:|aborted)/i.test(l))
    .filter((l) => !/^[+~\-→]\s|^\s*[a-z-]+:\s*(create|update)$/i.test(l))
    // "Change 'x' is invalid" only repeats the toast title. The line that says
    // what is actually wrong sits above it.
    .filter((l) => !/is (in)?valid\.?$/i.test(l))

  // Prefer an explicit error line wherever it sits; otherwise the last line
  // standing is the tool's closing word on what happened.
  const reason = lines.find((l) => /^error[: ]/i.test(l)) ?? lines[lines.length - 1]
  if (!reason) return undefined
  // Long enough to be a sentence, short enough to read without scrolling.
  return reason.length > 180 ? `${reason.slice(0, 179)}…` : reason
}

/**
 * Reports the result of an OpenSpec CLI run.
 *
 * The outcome is a three-arm union and only one arm means success. A missing
 * CLI comes back as `cliMissing` rather than a rejected promise, so anything
 * that treats "resolved" as "worked" would claim a change validated when the
 * CLI was never there -- or that it archived when nothing moved.
 *
 * Success says one sentence and nothing else: on the happy path the CLI's
 * transcript tells the user things they did not ask about and cannot act on.
 * Failure adds the single line that says why, because a failure the user cannot
 * diagnose is worse than no message.
 */
export function reportCliOutcome(
  outcome: CliOutcome,
  copy: { ok: string; failed: string }
): boolean {
  if (outcome.kind === 'cliMissing') {
    toast.warning('The OpenSpec tool is not installed.', {
      description: 'Install it with: npm install -g @fission-ai/openspec',
    })
    return false
  }
  if (outcome.kind === 'failed') {
    toast.error(copy.failed, { description: cliReason(outcome.output) })
    return false
  }
  toast.success(copy.ok)
  return true
}
