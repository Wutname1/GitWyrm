import type { SpecReturnTarget } from '@/lib/bindings'

/** One thing a finished chat can tell its spec, in the words the person reads. */
export interface SpecReturnChoice {
  id: SpecReturnTarget
  label: string
  detail: string
}

/**
 * What a finished chat can send back to its spec change.
 *
 * The order is how often each one is the right answer. The tasks list goes
 * stale first, because work either finishes a step or turns out to need one
 * nobody wrote down. The proposal and the design change less often, and only
 * when the work actually proved something about them.
 *
 * Each entry names a file, so the person knows what they are about to be
 * shown a draft of before they pick it: nothing here writes anything, and
 * being specific about the destination is what makes that believable.
 */
export const specReturnTargets: readonly SpecReturnChoice[] = [
  {
    id: 'tasks',
    label: 'Update the task list',
    detail: 'Tick off what got done, add a step the work turned out to need.',
  },
  {
    id: 'proposal',
    label: 'Update the proposal',
    detail: 'Correct what the work proved wrong, note what it revealed.',
  },
  {
    id: 'design',
    label: 'Update the design notes',
    detail: 'Record a decision the work forced, or a note it invalidated.',
  },
] as const
