import { plural } from '@/lib/gitDisplay'

/**
 * Which outcome the sync modal is previewing.
 *
 * `replace` and `reset` are the destructive pair and differ only in which side
 * loses work: `replace` (force-push) overwrites THEIR commits, `reset` throws
 * away OURS. `get` and `send` are the straight-line cases where only one side
 * has moved and there is no decision to make.
 */
export type PreviewMode = 'replace' | 'blend' | 'stack' | 'reset' | 'get' | 'send'

/** How the two refs stand against each other, in commits. */
export interface Divergence {
  /** Commits only our side has. */
  ours: number
  /** Commits only their side has. */
  theirs: number
}

/** Tone drives the color of the note and the "cost" pill. */
export type Tone = 'good' | 'warn' | 'bad' | 'plain'

export interface ModeCopy {
  mode: PreviewMode
  /** Short label for the mode button. */
  label: string
  /** The consequence, shown under the label -- "2 lost", "+1 commit", "tidy". */
  sub: string
  /** Text of the confirm button. */
  action: string
  /** One line under the graph saying what happens. */
  caption: string
  /** The pill beside the caption. */
  pill: { tone: Tone; text: string }
  /** The fuller explanation below the graph. */
  note: { tone: Tone; text: string }
  /** True for actions that destroy commits, which get red styling throughout. */
  danger: boolean
}

/**
 * Which modes apply to a given divergence.
 *
 * Both sides moved -> the real choice between replace / blend / stack. Only one
 * side moved -> a single straight-line action, because there is nothing to
 * reconcile and offering three buttons would imply a decision that isn't there.
 */
export function modesFor(d: Divergence): PreviewMode[] {
  if (d.ours > 0 && d.theirs > 0) return ['replace', 'blend', 'stack']
  if (d.ours === 0 && d.theirs > 0) return ['get']
  if (d.theirs === 0 && d.ours > 0) return ['send']
  return []
}

/**
 * Every string the modal shows for a mode, resolved against real counts.
 *
 * Kept here rather than in the component so the wording is testable and lives
 * in one place -- the copy is the part that decides whether someone understands
 * they are about to delete work.
 */
export function modeCopy(mode: PreviewMode, d: Divergence): ModeCopy {
  const ours = plural(d.ours, 'change')
  const theirs = plural(d.theirs, 'change')

  switch (mode) {
    case 'replace':
      return {
        mode,
        label: 'Replace',
        sub: `${d.theirs} lost`,
        action: 'Replace cloud copy',
        caption: `Their ${theirs} are deleted for good.`,
        pill: { tone: 'bad', text: `${d.theirs} lost` },
        note: {
          tone: 'bad',
          text: `Their ${theirs} are gone for good. Only do this if those changes aren't needed.`,
        },
        danger: true,
      }
    case 'reset':
      return {
        mode,
        label: 'Reset',
        sub: `${d.ours} lost`,
        action: 'Reset to match',
        caption: `Your ${ours} are thrown away.`,
        pill: { tone: 'bad', text: `${d.ours} lost` },
        note: {
          tone: 'bad',
          text: `Your ${ours} are gone for good. Only do this if those changes aren't needed.`,
        },
        danger: true,
      }
    case 'blend':
      return {
        mode,
        label: 'Blend',
        sub: '+1 commit',
        action: 'Blend them together',
        caption: 'Everything is kept, adds one blend commit.',
        pill: { tone: 'good', text: 'nothing lost' },
        note: {
          tone: 'good',
          text: 'Both sets of changes are kept, joined by one new blend commit. Nothing is lost.',
        },
        danger: false,
      }
    case 'stack':
      return {
        mode,
        label: 'Stack',
        sub: 'tidy',
        action: 'Stack on top',
        caption: `Your ${ours} are rebuilt on top of theirs.`,
        pill: { tone: 'good', text: 'nothing lost' },
        note: {
          // Nobody pulls your local branch -- the risk is that these commits
          // were already sent up, so rewriting them makes the cloud copy differ.
          tone: 'warn',
          text: `Your ${ours} get new IDs. If you already sent them up, the cloud copy will no longer match.`,
        },
        danger: false,
      }
    case 'get':
      return {
        mode,
        label: 'Get',
        sub: 'clean',
        action: `Get ${theirs}`,
        caption: `${theirs} come down. Nothing of yours is in the way.`,
        pill: { tone: 'good', text: 'nothing lost' },
        note: { tone: 'plain', text: 'A straight catch-up. Your branch simply moves up to match the cloud.' },
        danger: false,
      }
    case 'send':
      return {
        mode,
        label: 'Send up',
        sub: 'clean',
        action: `Send ${ours} up`,
        caption: `${ours} go up. The cloud has nothing you don't.`,
        pill: { tone: 'good', text: 'nothing lost' },
        note: { tone: 'plain', text: 'A straight publish. The cloud copy moves up to match you.' },
        danger: false,
      }
  }
}

/**
 * Whether pressing Pull should open the sync modal instead of pulling.
 *
 * A plain `git pull` on a branch that has moved on BOTH sides silently writes a
 * merge commit -- a permanent history decision taken on the user's behalf, and
 * the exact thing the sync modal exists to hand back. When only one side moved
 * there is nothing to decide, so the pull runs straight through.
 *
 * Needs an upstream because the modal pairs the branch against that exact ref.
 */
export function pullNeedsChoice(branch: {
  upstream?: string | null
  ahead: number
  behind: number
}): boolean {
  return !!branch.upstream && branch.ahead > 0 && branch.behind > 0
}

/** Geometry shared by the before/after schematic, so both halves line up. */
export const GRAPH = {
  /** Vertical distance between commits on a lane. */
  gap: 28,
  /** Y of the lowest commit on an arm. */
  baseDot: 122,
  /** Y of the shared-base dot. */
  root: 170,
  /** Most dots drawn per arm; the label carries the true count. */
  maxDots: 3,
} as const

/** How many dots to draw for `n` commits. */
export const dotCount = (n: number) => Math.min(n, GRAPH.maxDots)

/** Y of the topmost dot on an arm holding `n` commits. */
export const armTop = (n: number) => GRAPH.baseDot - (dotCount(n) - 1) * GRAPH.gap

/**
 * Y positions for a single lane holding `total` commits, squeezed so the stack
 * never runs past `topLimit`.
 *
 * The stacked (rebase) result is the tallest shape the graph draws -- both arms
 * on one lane -- so at 3 up / 40 down a fixed gap would push the top commit and
 * its label off the canvas and behind the heading.
 */
export function stackedLane(total: number, baseY = 156, topLimit = 44) {
  const gap = Math.min(GRAPH.gap, (baseY - topLimit) / Math.max(total, 1))
  return { gap, y: (i: number) => baseY - i * gap }
}
