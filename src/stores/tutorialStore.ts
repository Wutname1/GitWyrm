import { create } from 'zustand'
import { commands } from '@/lib/bindings'
import { log } from '@/lib/log'
import { unwrap } from '@/lib/queryKeys'

/** How the tutorial ended, so the closing card can say the right thing. */
export type TutorialOutcome = 'finished' | 'exited' | null

interface TutorialState {
  /** True from "yes, show me" until the practice repo is open and lesson 1 is up. */
  starting: boolean
  /** True while a lesson is on screen. Drives the overlay. */
  active: boolean
  /** Index into the lesson list. */
  step: number
  /**
   * Lessons the user actually performed, as opposed to skipped. Kept so the
   * closing card can say "you did 4 of 5" rather than implying they did it all.
   */
  completed: Set<number>
  /**
   * Bumped the moment a lesson's gesture is detected, before the step advances.
   * The overlay watches it to play the success beat, which is what makes the
   * gesture feel acknowledged (Rule #1) instead of the panel just moving on.
   */
  successNonce: number
  /** Practice repo path and its diverged branch, once seeded. */
  repoPath: string | null
  featureBranch: string | null
  /** Repo id of the practice tab, so it can be closed on the way out. */
  repoId: string | null
  outcome: TutorialOutcome
  /** Set when seeding fails, so the UI can explain rather than hang. */
  error: string | null

  /** Seed the practice repo and open lesson one. */
  start: (openRepo: (path: string) => Promise<string>) => Promise<void>
  /** Mark the current lesson done and move on (or finish). */
  completeStep: (total: number) => void
  /** Move on without crediting the lesson. */
  skipStep: (total: number) => void
  /** Leave the tutorial early. */
  exit: () => void
  /** Tear down the practice repo. Safe to call twice. */
  cleanup: (closeRepo: (repoId: string) => Promise<void>) => Promise<void>
  /** Clear the closing card. */
  dismissOutcome: () => void
}

export const useTutorialStore = create<TutorialState>((set, get) => ({
  starting: false,
  active: false,
  step: 0,
  completed: new Set(),
  successNonce: 0,
  repoPath: null,
  featureBranch: null,
  repoId: null,
  outcome: null,
  error: null,

  start: async (openRepo) => {
    set({ starting: true, error: null, outcome: null })
    try {
      const seeded = unwrap(await commands.createTutorialRepo())
      const repoId = await openRepo(seeded.path)
      set({
        starting: false,
        active: true,
        step: 0,
        completed: new Set(),
        repoPath: seeded.path,
        featureBranch: seeded.feature_branch,
        repoId,
      })
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error)
      log.error(`Could not start the tutorial: ${message}`)
      // Surfaced rather than swallowed: the user asked for the tour and would
      // otherwise be staring at a spinner that never resolves.
      set({ starting: false, active: false, error: message })
    }
  },

  completeStep: (total) =>
    set((s) => {
      const completed = new Set(s.completed).add(s.step)
      const next = s.step + 1
      const done = next >= total
      return {
        completed,
        successNonce: s.successNonce + 1,
        step: done ? s.step : next,
        active: !done,
        outcome: done ? 'finished' : null,
      }
    }),

  skipStep: (total) =>
    set((s) => {
      const next = s.step + 1
      const done = next >= total
      return {
        step: done ? s.step : next,
        active: !done,
        outcome: done ? 'finished' : null,
      }
    }),

  exit: () => set({ active: false, outcome: 'exited' }),

  cleanup: async (closeRepo) => {
    const { repoId } = get()
    // Close the tab before deleting the folder: a repo still open has a file
    // watcher on it, and removing the directory underneath that is what turns a
    // tidy exit into an error toast about a path that no longer exists.
    if (repoId) {
      try {
        await closeRepo(repoId)
      } catch (error) {
        log.warn(`Could not close the practice repo tab: ${String(error)}`)
      }
    }
    try {
      unwrap(await commands.discardTutorialRepo())
    } catch (error) {
      log.warn(`Could not remove the practice repo: ${String(error)}`)
    }
    set({ repoId: null, repoPath: null, featureBranch: null })
  },

  dismissOutcome: () => set({ outcome: null, error: null }),
}))
