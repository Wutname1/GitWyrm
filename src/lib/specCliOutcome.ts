import { toast } from 'sonner'
import type { CliOutcome } from '@/lib/bindings'

/**
 * Reports the result of an OpenSpec CLI run.
 *
 * The outcome is a three-arm union and only one arm means success. A missing
 * CLI comes back as `cliMissing` rather than a rejected promise, so anything
 * that treats "resolved" as "worked" would claim a change validated when the
 * CLI was never there -- or that it archived when nothing moved.
 *
 * The CLI's own text goes in the toast description rather than the title: it is
 * raw multi-line output, and the title has to stay a plain-language sentence.
 */
export function reportCliOutcome(
  outcome: CliOutcome,
  copy: { ok: string; failed: string }
): boolean {
  if (outcome.kind === 'cliMissing') {
    toast.warning('The OpenSpec command line tool is not installed.', {
      description: outcome.hint,
    })
    return false
  }
  if (outcome.kind === 'failed') {
    toast.error(copy.failed, { description: outcome.output.trim() || undefined })
    return false
  }
  toast.success(copy.ok, { description: outcome.output.trim() || undefined })
  return true
}
