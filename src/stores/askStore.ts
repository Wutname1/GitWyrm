import { create } from 'zustand'
import type { AskAnswer } from '@/lib/bindings'

/** One turn in an ask conversation. */
export interface AskTurn {
  id: number
  question: string
  /** Null while the answer is still coming back. */
  answer: AskAnswer | null
  /** Set when the request failed, so the bubble can say so in place. */
  error: string | null
}

interface AskSession {
  changeId: string
  turns: AskTurn[]
  pending: boolean
}

interface AskState {
  /** One session per repo, mirroring how runs are keyed. */
  byRepo: Record<string, AskSession | undefined>
  /**
   * Bumped whenever a session is started or cleared. In-flight replies carry the
   * value they started with and are dropped if it moved, which is what stops a
   * stale answer from landing in a conversation the user has already left.
   */
  epoch: number
  start: (repoId: string, changeId: string) => number
  ask: (repoId: string, question: string) => number
  settle: (
    repoId: string,
    turnId: number,
    epoch: number,
    result: { answer?: AskAnswer; error?: string }
  ) => void
  clear: (repoId: string) => void
}

let nextTurnId = 1

export const useAskStore = create<AskState>((set, get) => ({
  byRepo: {},
  epoch: 0,

  /**
   * Begin (or switch to) an ask session for a change. Starting a session for a
   * different change replaces the conversation rather than appending to it: the
   * answers were grounded in a different set of documents, and mixing them would
   * make the citations lie.
   */
  start: (repoId, changeId) => {
    const existing = get().byRepo[repoId]
    if (existing?.changeId === changeId) return get().epoch
    const epoch = get().epoch + 1
    set((s) => ({
      epoch,
      byRepo: { ...s.byRepo, [repoId]: { changeId, turns: [], pending: false } },
    }))
    return epoch
  },

  /** Add a question and mark the session busy. Returns the new turn's id. */
  ask: (repoId, question) => {
    const session = get().byRepo[repoId]
    if (!session) return -1
    const id = nextTurnId++
    set((s) => ({
      byRepo: {
        ...s.byRepo,
        [repoId]: {
          ...session,
          turns: [...session.turns, { id, question, answer: null, error: null }],
          pending: true,
        },
      },
    }))
    return id
  },

  /** Fill in an answer, unless the session moved on while it was in flight. */
  settle: (repoId, turnId, epoch, result) => {
    if (get().epoch !== epoch) return
    const session = get().byRepo[repoId]
    if (!session) return
    set((s) => ({
      byRepo: {
        ...s.byRepo,
        [repoId]: {
          ...session,
          pending: false,
          turns: session.turns.map((t) =>
            t.id === turnId
              ? { ...t, answer: result.answer ?? null, error: result.error ?? null }
              : t
          ),
        },
      },
    }))
  },

  clear: (repoId) =>
    set((s) => ({
      epoch: s.epoch + 1,
      byRepo: { ...s.byRepo, [repoId]: undefined },
    })),
}))
