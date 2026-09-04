import { create } from 'zustand'

/**
 * Unsaved spec-file edits, held while the Desk is open.
 *
 * This lives outside the components because the editor is unmounted whenever the
 * user switches tabs or selects another change. Component state would drop the
 * text on the first click elsewhere, which is the single worst thing an editor
 * can do. Drafts are deliberately *not* persisted to disk: the promise is that
 * they survive moving around the Desk, not that they outlive the window --
 * anything stronger would mean an invisible copy of a file competing with what
 * git and other tools see.
 */

/**
 * A draft is keyed by repo, change and file: three different files can be
 * mid-edit at once.
 *
 * Exported, and the only place this string is built. Components subscribing to
 * one draft need the same key the store writes under, and when they built it
 * themselves the two drifted apart -- the store wrote under one separator and
 * the component read another, so every lookup missed and the editor opened
 * empty with the text sitting in the store the whole time. One function, one
 * format, no way to disagree.
 */
export function draftKey(repoId: string, changeId: string, file: string) {
  return `${repoId} ${changeId} ${file}`
}

/** Key prefix for every draft belonging to one change. */
export function changeDraftPrefix(repoId: string, changeId: string) {
  return `${repoId} ${changeId} `
}

export interface SpecDraft {
  /** What the editor currently holds. */
  text: string
  /** What was on disk when editing started, to tell a real edit from a no-op. */
  original: string
}

/**
 * An AI-drafted edit waiting to be looked at, with the file's contents as they
 * were when it was drafted so the diff compares against what the person saw.
 *
 * Held in the store rather than in `DeskDetail`'s own state because two
 * different places produce one: Spec Desk's own Ask AI, and Agent Desk's "Tell
 * the spec". While this was component-local, Agent Desk could not reach it, so
 * its drafts skipped the diff review entirely and went straight into the
 * editor -- the same class of generated text reaching the same files under two
 * different safety contracts.
 */
export interface PendingSpecEdit {
  repoId: string
  changeId: string
  file: string
  /** What the AI proposes the file should become. */
  body: string
  /** The AI's one-line description of what it changed. */
  summary: string
  /** What was on disk when it was drafted, so the diff is against what the person saw. */
  current: string
}

interface SpecDraftState {
  drafts: Record<string, SpecDraft | undefined>
  /** An AI edit awaiting Accept/Reject, or `null`. At most one at a time. */
  pendingEdit: PendingSpecEdit | null
  /** Offer an AI edit for review. Never writes; the person accepts or rejects. */
  proposeEdit: (edit: PendingSpecEdit) => void
  /** Clear the pending edit, whether it was accepted or rejected. */
  clearPendingEdit: () => void
  /** Start editing, or resume an existing draft for this file. */
  open: (repoId: string, changeId: string, file: string, onDisk: string) => void
  edit: (repoId: string, changeId: string, file: string, text: string) => void
  /** Replace the text wholesale -- an accepted AI draft landing in the editor. */
  replace: (repoId: string, changeId: string, file: string, text: string) => void
  discard: (repoId: string, changeId: string, file: string) => void
  get: (repoId: string, changeId: string, file: string) => SpecDraft | undefined
  /** True when this file has edits that differ from what was read off disk. */
  isDirty: (repoId: string, changeId: string, file: string) => boolean
  /** Any unsaved file in this change, for the marker on the change itself. */
  changeHasDirty: (repoId: string, changeId: string) => boolean
}

export const useSpecDraftStore = create<SpecDraftState>((set, get) => ({
  drafts: {},
  pendingEdit: null,

  proposeEdit: (edit) => set({ pendingEdit: edit }),
  clearPendingEdit: () => set({ pendingEdit: null }),

  open: (repoId, changeId, file, onDisk) => {
    const key = draftKey(repoId, changeId, file)
    // An existing draft wins: reopening a file the user was already editing
    // must not silently throw their unfinished text away. `original` is left
    // as it was, so dirtiness is still measured against the same baseline.
    if (get().drafts[key]) return
    set((s) => ({ drafts: { ...s.drafts, [key]: { text: onDisk, original: onDisk } } }))
  },

  edit: (repoId, changeId, file, text) => {
    const key = draftKey(repoId, changeId, file)
    set((s) => {
      const existing = s.drafts[key]
      if (!existing) return s
      return { drafts: { ...s.drafts, [key]: { ...existing, text } } }
    })
  },

  replace: (repoId, changeId, file, text) => {
    const key = draftKey(repoId, changeId, file)
    set((s) => {
      const existing = s.drafts[key]
      // Accepting a draft for a file that is not open starts one, using the
      // AI's text as the edit and leaving `original` empty-but-known so the
      // result reads as unsaved rather than as already-saved.
      const original = existing?.original ?? ''
      return { drafts: { ...s.drafts, [key]: { text, original } } }
    })
  },

  discard: (repoId, changeId, file) => {
    const key = draftKey(repoId, changeId, file)
    set((s) => {
      const next = { ...s.drafts }
      delete next[key]
      return { drafts: next }
    })
  },

  get: (repoId, changeId, file) => get().drafts[draftKey(repoId, changeId, file)],

  isDirty: (repoId, changeId, file) => {
    const draft = get().drafts[draftKey(repoId, changeId, file)]
    return draft != null && draft.text !== draft.original
  },

  changeHasDirty: (repoId, changeId) => {
    const prefix = changeDraftPrefix(repoId, changeId)
    return Object.entries(get().drafts).some(
      ([key, draft]) => key.startsWith(prefix) && draft != null && draft.text !== draft.original
    )
  },
}))
