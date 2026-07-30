import { toast } from 'sonner'
import type { SpecChange, SpecTask } from '@/lib/bindings'

/**
 * Composes the handoff text for one task and puts it on the clipboard.
 *
 * "Handoff", never "prompt": it is a briefing a person could read, and it is
 * what an external tool (opencode, VS Code, any AI chat) needs in order to do
 * exactly one task without re-deriving the plan. The rules block is the part
 * that keeps a tool from wandering: one task, tick it, validate, leave the rest
 * alone.
 *
 * Editable templates land with the Spec Desk's action rail; this is the shared
 * composer both windows call so they can never disagree about what "the
 * handoff" is.
 */
export function composeTaskHandoff(change: SpecChange, task: SpecTask | undefined): string {
  const readFirst = [
    `- openspec/changes/${change.id}/proposal.md`,
    ...(change.has_design ? [`- openspec/changes/${change.id}/design.md`] : []),
    ...change.deltas.map((d) => `- ${d.file} (${d.kind.toLowerCase()})`),
  ].join('\n')

  if (!task) {
    return [
      `## OpenSpec change: ${change.id}`,
      '',
      'Every task is done. Review the change before it is archived.',
      '',
      'Read first',
      readFirst,
      '',
      'Rules',
      `- Check the work matches the requirements above.`,
      `- Run: openspec validate ${change.id}`,
      '- Do not start new work.',
    ].join('\n')
  }

  const number = change.tasks.indexOf(task) + 1
  return [
    `## OpenSpec change: ${change.id}`,
    `Do task ${number} of ${change.tasks.length}: ${task.text}`,
    '',
    'Read first',
    readFirst,
    '',
    'Rules',
    '- Only this task.',
    `- Mark it done in openspec/changes/${change.id}/tasks.md: - [x]`,
    `- Run: openspec validate ${change.id}`,
    '- Keep unrelated work intact.',
  ].join('\n')
}

/**
 * Copies a task handoff and confirms it, because a clipboard write is invisible
 * otherwise. Says where to paste it rather than just "copied".
 *
 * `silent` suppresses only the success toast, for callers that copy as one step
 * of a larger action and report that instead -- two toasts for one click reads
 * as a stutter. A failure still speaks up: the caller is about to tell the user
 * the handoff is on the clipboard, and it would not be.
 */
export async function copyTaskHandoff(
  change: SpecChange,
  task: SpecTask | undefined,
  options?: { silent?: boolean }
) {
  const text = composeTaskHandoff(change, task)
  try {
    await navigator.clipboard.writeText(text)
    if (options?.silent) return
    toast.success(
      task
        ? 'Handoff copied - paste it into opencode, VS Code, or any AI chat.'
        : 'Review handoff copied - paste it into opencode, VS Code, or any AI chat.'
    )
  } catch {
    // Clipboard permission is the only realistic failure here, and silently
    // doing nothing would look like a broken button.
    toast.error('Could not copy the handoff. Check clipboard permissions.')
  }
}
