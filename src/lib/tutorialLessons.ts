import type { ReactNode } from 'react'

/**
 * What the ghost animation should mime for a lesson. Kept as a small vocabulary
 * rather than free-form animation data so every lesson demonstrates its gesture
 * the same recognisable way.
 */
export type GestureKind = 'double-click' | 'drag' | 'modified-click' | 'right-click'

/**
 * Signals the app raises when something teachable happens. A lesson finishes
 * when its `completeOn` signal arrives, which means the user genuinely did the
 * gesture -- not that they clicked "Next".
 */
export type TutorialSignal =
  | 'branch-switched'
  | 'sync-opened'
  | 'multi-selected'
  | 'context-menu-opened'
  | 'lines-selected'

/**
 * Branch names the practice repo is seeded with, mirrored from
 * `src-tauri/src/commands/tutorial.rs`. The lessons spotlight rows by name, so
 * these two have to agree; if the Rust seed changes, change these with it.
 */
export const TUTORIAL_MAIN_BRANCH = 'main'
export const TUTORIAL_FEATURE_BRANCH = 'feature/lamp-upgrade'

/**
 * Spotlight id for a sidebar branch row, or undefined for branches no lesson
 * points at. Returning undefined keeps the attribute off every other row rather
 * than tagging the whole sidebar.
 */
export function tutorialBranchId(name: string): string | undefined {
  if (name === TUTORIAL_FEATURE_BRANCH) return 'sidebar-feature-branch'
  if (name === TUTORIAL_MAIN_BRANCH) return 'sidebar-main-branch'
  return undefined
}

/**
 * Spotlight id for a commit row by its position in the graph.
 *
 * Only the top two rows are tagged, and the multi-select lesson lights both:
 * "click a second commit" is a hard instruction to follow when the spotlight
 * only admits one row. Indexed by position rather than sha because the practice
 * repo is rebuilt on every run, so its shas are never the same twice.
 */
export function tutorialCommitId(index: number): string | undefined {
  if (index === 0) return 'graph-commit-row'
  if (index === 1) return 'graph-commit-row-second'
  return undefined
}

/**
 * The practice repo's edited file, left uncommitted by the seed so the staging
 * lesson has something real to open. Matches `NOTES_WORKING` in
 * `src-tauri/src/commands/tutorial.rs`.
 */
export const TUTORIAL_DIRTY_FILE = 'notes.md'

/** Spotlight id for the practice repo's changed file row. */
export function tutorialFileId(path: string): string | undefined {
  return path === TUTORIAL_DIRTY_FILE ? 'changes-file-row' : undefined
}

export interface Lesson {
  id: string
  /**
   * `data-tutorial-id`s to leave lit. The first is where the gesture starts and
   * is what the coach card anchors to; any others are part of the same gesture
   * (a drag's destination, the second commit in a range) and would make the
   * instruction impossible to follow if they stayed dimmed.
   */
  targets: string[]
  title: string
  body: ReactNode
  /** One short line naming the gesture, shown under the body. */
  instruction: string
  gesture: GestureKind
  completeOn: TutorialSignal
  /** Shown for a beat when the gesture lands. */
  success: string
}

/**
 * The hands-on lessons, ordered by how much they unlock.
 *
 * The list is deliberately short. Every one of these is a gesture with no
 * visible affordance at rest -- there is no button, no menu entry, and nothing
 * on screen that hints it exists -- so they are the things a user genuinely
 * cannot find alone. Anything already reachable from a toolbar button or a
 * visible menu is left out: teaching it would pad the tour without adding
 * anything the user would not have found in a minute of clicking.
 */
export const LESSONS: Lesson[] = [
  {
    id: 'switch-branch',
    targets: ['sidebar-feature-branch'],
    title: 'Double-click to switch branches',
    body: 'Your work lives on branches. This practice repo has two. Double-clicking a branch moves you onto it, so the files you see become that branch’s files.',
    instruction: 'Double-click the highlighted branch.',
    gesture: 'double-click',
    completeOn: 'branch-switched',
    success: 'You switched branches.',
  },
  {
    id: 'drag-sync',
    targets: ['sidebar-feature-branch', 'sidebar-main-branch'],
    title: 'Drag one branch onto another',
    body: 'These two branches have both moved on, so each has work the other is missing. Dragging one onto the other asks GitWyrm to sort that out, and it offers only what actually makes sense for the pair.',
    instruction: 'Drag the highlighted branch onto the one below it.',
    gesture: 'drag',
    completeOn: 'sync-opened',
    success: 'That is the sync panel. Close it whenever you like.',
  },
  {
    id: 'multi-select',
    targets: ['graph-commit-row', 'graph-commit-row-second'],
    title: 'Select more than one commit',
    body: 'Hold Shift and click a second commit to grab everything between the two. Hold Ctrl instead to add or remove one at a time. With several picked, right-clicking offers actions that work on the whole group.',
    instruction: 'Hold Shift and click a second commit in the list.',
    gesture: 'modified-click',
    completeOn: 'multi-selected',
    success: 'Both commits are selected.',
  },
  {
    id: 'right-click',
    targets: ['graph-commit-row'],
    title: 'Right-click almost anything',
    body: 'Commits, branches, tags, tabs, files, and stashes all carry their own right-click menu. It is the fastest route to most of what GitWyrm can do, and it is worth a try whenever you are looking for an action.',
    instruction: 'Right-click a commit in the list.',
    gesture: 'right-click',
    completeOn: 'context-menu-opened',
    success: 'That menu changes depending on what you right-click.',
  },
  {
    id: 'line-staging',
    targets: ['changes-file-row'],
    title: 'Commit only part of a file',
    body: 'Open a changed file and you can click individual lines, then Shift-click to take a run of them. Right-click the ones you picked to commit just those and leave the rest for later.',
    instruction: 'Click the highlighted file to open its changes.',
    gesture: 'modified-click',
    completeOn: 'lines-selected',
    success: 'Nice — that is the last one.',
  },
]
