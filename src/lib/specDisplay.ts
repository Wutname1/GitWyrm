import type { ChangeStatus, SpecChange } from '@/lib/bindings'

/**
 * Plain-language labels for a change's derived status.
 *
 * The backend derives the status; this is the only place it becomes words, so
 * the sidebar, the spec card, and (later) the Spec Desk cannot describe the same
 * change differently.
 */
export const STATUS_LABEL: Record<ChangeStatus, string> = {
  draft: 'Draft',
  inBuild: 'In build',
  needsReview: 'Needs review',
  readyToArchive: 'Ready to archive',
}

/** Token driving a status pill's color. Semantic, not the accent hue. */
export function statusTone(status: ChangeStatus): 'muted' | 'info' | 'warn' | 'good' {
  switch (status) {
    case 'draft':
      return 'muted'
    case 'inBuild':
      return 'info'
    case 'needsReview':
      return 'warn'
    case 'readyToArchive':
      return 'good'
  }
}

/**
 * The short count a sidebar row shows: `7/10`, or `draft` when a change has no
 * tasks yet. Never "0/0", which reads as broken rather than as unstarted.
 */
export function progressCount(change: SpecChange): string {
  const { done, total, is_draft } = change.progress
  return is_draft ? 'draft' : `${done}/${total}`
}

/**
 * "7 of 10 tasks done" for the spec card, in words rather than a fraction.
 * Singular/plural is handled because "1 tasks" is the kind of detail that makes
 * an app feel unfinished.
 */
export function progressSentence(change: SpecChange): string {
  const { done, total, is_draft } = change.progress
  if (is_draft) return 'No tasks yet'
  return `${done} of ${total} ${total === 1 ? 'task' : 'tasks'} done`
}

/**
 * What to do next with this change, as a sentence. Mirrors the mockup's hint
 * line: it answers "what happens now?", never just restating the status.
 */
export function nextStepHint(change: SpecChange): string {
  const { done, total, is_draft } = change.progress
  if (is_draft || total === 0) return "Add tasks to tasks.md to get started - it's plain markdown."
  if (done === total) {
    return change.deltas.length > 0
      ? 'Every task is done. Run the spec check, then archive it.'
      : 'Every task is done, but there are no requirements to archive yet.'
  }
  const open = total - done
  return `${open} ${open === 1 ? 'task remains' : 'tasks remain'}.`
}
