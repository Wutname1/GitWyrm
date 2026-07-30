import { create } from 'zustand'
import type { RunEventKind, RunSession, RunState, RunStep } from '@/lib/bindings'

/**
 * One run per repository, shared by every surface that shows it.
 *
 * This is a store rather than component state because five places render the
 * same run -- the AI tab, its badge, the Desk rail banner, the status bar, and
 * the spec card. With `useState` in a hook, each caller got its own copy, so
 * starting a run updated only the component that started it and every mirror
 * stayed blank. Caught on screen, not by the typechecker: five wrong copies
 * type exactly like one right one.
 */
interface RunEntry {
  session: RunSession | null
  steps: RunStep[]
  state: RunState | null
  latest: string
}

const EMPTY: RunEntry = { session: null, steps: [], state: null, latest: '' }

interface AiRunStore {
  byRepo: Record<string, RunEntry>
  /** Replaces a repository's run, e.g. after starting one or loading on mount. */
  setRun: (repoId: string, entry: RunEntry) => void
  /** Appends an event, ignoring anything from a superseded session. */
  applyEvent: (event: RunEventKind) => void
  clearRun: (repoId: string) => void
}

export const useAiRunStore = create<AiRunStore>((set) => ({
  byRepo: {},
  setRun: (repoId, entry) =>
    set((s) => ({ byRepo: { ...s.byRepo, [repoId]: entry } })),
  applyEvent: (event) =>
    set((s) => {
      const current = s.byRepo[event.repo_id]
      // The backend drops stale events too; this is the same rule on the near
      // side, so one cannot slip in between a start and a reload.
      if (current?.session && current.session.session_id !== event.session_id) {
        return s
      }
      const base = current ?? EMPTY
      return {
        byRepo: {
          ...s.byRepo,
          [event.repo_id]: {
            ...base,
            state: event.state,
            latest: event.summary,
            steps: [...base.steps, event.step],
          },
        },
      }
    }),
  clearRun: (repoId) =>
    set((s) => {
      const next = { ...s.byRepo }
      delete next[repoId]
      return { byRepo: next }
    }),
}))

export function selectRun(repoId: string | null): RunEntry {
  const byRepo = useAiRunStore.getState().byRepo
  return (repoId ? byRepo[repoId] : null) ?? EMPTY
}

export { EMPTY as EMPTY_RUN }
export type { RunEntry }
