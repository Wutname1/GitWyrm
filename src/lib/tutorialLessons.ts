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
  | 'sync-closed'
  | 'multi-selected'
  | 'context-menu-opened'

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
 * Spotlight id for a branch pill in the graph.
 *
 * The same two branches are lit in the sidebar, so the first two lessons can
 * show both places the gesture works at once -- the point being that these are
 * gestures on a branch, not on one particular list. Tags are skipped: they are
 * not branches and do not switch.
 */
export function tutorialRefPillId(name: string, type: string): string | undefined {
  if (type === 'tag') return undefined
  if (name === TUTORIAL_FEATURE_BRANCH) return 'pill-feature-branch'
  if (name === TUTORIAL_MAIN_BRANCH) return 'pill-main-branch'
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

/**
 * True for the tutorial's practice repository.
 *
 * Matched on the folder names the Rust seed creates rather than an exact path,
 * because the app-data root differs per machine and the frontend never learns
 * it. Anything under a real project called "practice-repo" would have to sit
 * inside a "tutorial" folder too, which is specific enough not to misfire.
 */
export function isTutorialRepoPath(path: string): boolean {
  const normalized = path.replace(/\\/g, '/').toLowerCase()
  return normalized.includes('/tutorial/practice-repo')
}

export interface Lesson {
  id: string
  /**
   * `data-tutorial-id`s to leave undimmed and clickable. The first anchors the
   * coach card. Use a container id when the gesture needs room to move -- a
   * drag that crosses a blocked band is cancelled mid-way, and a "click two of
   * these" instruction cannot be followed if only two are exposed.
   */
  targets: string[]
  /**
   * Which targets to draw the pulsing ring around. Defaults to all of them.
   * Set this when a target is a whole panel: the panel needs to stay live, but
   * ringing it would put a large pulsing rectangle around half the window
   * instead of pointing at anything.
   *
   * Each entry is a *group* of ids sharing one ring, drawn around their
   * combined bounds. Two branch rows sitting one above the other read as a
   * single "these two" far better than two separate outlines, and four rings
   * for what is really two places is just noise.
   */
  ringTargets?: string[][]
  title: string
  body: ReactNode
  /** One short line naming the gesture, shown under the body. */
  instruction: string
  gesture: GestureKind
  /**
   * Indexes into `targets` the ghost drag runs between. Explicit because a
   * lesson lights targets for several reasons -- a container kept live, the
   * same branch shown in two places -- and none of those imply a drag path.
   * `dragFromIndex` defaults to 0 when omitted.
   */
  dragFromIndex?: number
  dropTargetIndex?: number
  /**
   * A second drag path to mime at the same time, for a gesture that works in
   * two places. Both animations run together so neither location reads as the
   * only one that counts.
   */
  secondaryDragFromIndex?: number
  secondaryDropTargetIndex?: number
  /**
   * Dock the card against a side panel rather than placing it near the target.
   * Use it whenever the lesson's result shows up in the middle of the screen
   * (a context menu, a modal) or the gesture spans several places -- the card
   * must not land on top of the thing the user was told to open.
   */
  dock?: 'left' | 'right'
  /**
   * A second beat that runs after the gesture lands, for a gesture whose
   * result is a panel the user then has to read.
   *
   * The drag lesson opens the Sync panel, and stopping there left the tour
   * congratulating the user over the top of the very thing it had just told
   * them to open -- unreadable, and unusable, because the coach card sits above
   * the modal. During a follow-up the overlay stands down entirely: no scrim,
   * no ring, no card in the way. The lesson finishes when `until` fires.
   */
  followUp?: {
    title: string
    body: string
    /** What ends the follow-up and completes the lesson. */
    until: TutorialSignal
  }
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
    targets: ['sidebar-feature-branch', 'pill-feature-branch'],
    // The work is on the left (sidebar row) and centre (chip), so the card
    // lives on the right, over the changes panel.
    dock: 'right',
    title: 'Double-click to switch branches',
    body: 'Your work lives on branches. This practice repo has two. Double-clicking a branch moves you onto it, so the files you see become that branch’s files. Both spots below are the same branch: the row in the list on the left, and the tag on the commit itself.',
    instruction: 'Double-click either highlighted branch — the row or the tag.',
    gesture: 'double-click',
    completeOn: 'branch-switched',
    success: 'You switched branches.',
  },
  {
    id: 'drag-sync',
    // The whole sidebar stays live so the drag has somewhere to travel. Two
    // tight cutouts meant the pointer crossed a blocked band on the way
    // between them, which cancels the drag mid-gesture.
    // The sidebar and the graph both stay live so the drag has room to travel,
    // and all four branch markers are ringed: the same two branches appear as
    // rows on the left and as chips on the commits, and the gesture works
    // between any of them.
    targets: [
      'branch-sidebar',
      'graph-commit-list',
      'sidebar-feature-branch',
      'sidebar-main-branch',
      'pill-feature-branch',
      'pill-main-branch',
    ],
    // Two rings, not four: the pair of sidebar rows, and the pair of chips.
    ringTargets: [
      ['sidebar-feature-branch', 'sidebar-main-branch'],
      ['pill-feature-branch', 'pill-main-branch'],
    ],
    // Same reason as above, plus the Sync panel opens dead centre.
    dock: 'right',
    title: 'Drag one branch onto another',
    body: 'These two branches have both moved on, so each has work the other is missing. Dragging one onto the other asks GitWyrm to sort that out, and it offers only what makes sense for the pair. Every glowing spot is one of those two branches — the rows on the left, or the tags on the commits.',
    instruction: 'Drag either branch onto the other one.',
    gesture: 'drag',
    // Indexes into `targets`: the feature row (2) onto the main row (3), and
    // the same pairing again for the chips (4 -> 5), so the mime plays in both
    // places at once and neither reads as the only way to do it.
    dragFromIndex: 2,
    dropTargetIndex: 3,
    secondaryDragFromIndex: 4,
    secondaryDropTargetIndex: 5,
    completeOn: 'sync-opened',
    success: 'That opened the sync panel.',
    followUp: {
      title: 'This is the sync panel',
      body: 'It shows the two branches, which way the work will move, and what will happen when it does. GitWyrm only offers what actually makes sense for these two, so there is nothing here that can catch you out. Have a look, then close it — you do not have to sync anything right now.',
      until: 'sync-closed',
    },
  },
  {
    id: 'multi-select',
    // The whole list, not two rows: the lesson asks for a click and then a
    // second click somewhere else, so pre-picking which rows are allowed makes
    // the instruction impossible to follow when the user starts anywhere but
    // the one row that was lit.
    targets: ['graph-commit-list'],
    // The user is clicking around the whole commit list, so the card keeps out
    // of it entirely rather than sitting on rows they may need.
    dock: 'left',
    title: 'Select more than one commit',
    body: 'Click any commit, then hold Shift and click another to grab everything between the two. Hold Ctrl instead to add or remove one at a time. With several picked, right-clicking offers actions that work on the whole group.',
    instruction: 'Click one commit, then Shift-click another.',
    gesture: 'modified-click',
    completeOn: 'multi-selected',
    success: 'Both commits are selected.',
  },
  {
    id: 'right-click',
    targets: ['graph-commit-list'],
    // The menu opens next to whichever commit is clicked, anywhere in the
    // graph, so the card sits fully outside it over the branch sidebar.
    dock: 'left',
    title: 'Right-click a commit for its actions',
    body: 'Right-clicking a commit opens a menu of everything you can do to it — undo it, copy it to another branch, rename its message, or tag it. Branches, tags, tabs, files and stashes each have their own menu too, so it is worth a try whenever you are hunting for an action.',
    instruction: 'Right-click any commit in the list.',
    gesture: 'right-click',
    completeOn: 'context-menu-opened',
    success:
      'Have a read of what is in there, then close it with Escape. The same trick works on branches, tags and files.',
  },
]
