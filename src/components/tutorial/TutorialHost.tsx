import { useEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { Hand } from 'lucide-react'
import { cn } from '@/lib/utils'
import { LESSONS, type Lesson, type TutorialSignal } from '@/lib/tutorialLessons'
import { useBranches } from '@/hooks/useGitQueries'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo } from '@/stores/workspaceStore'
import { useTutorialStore } from '@/stores/tutorialStore'
import { TutorialOverlay } from './TutorialOverlay'
import { TutorialCoach } from './TutorialCoach'
import { scrollTutorialTargetIntoView } from './useTutorialTarget'

/**
 * How long the success line stays up before the next lesson takes over.
 *
 * Short lines are pure acknowledgement ("You switched branches.") -- the user
 * already saw the branch change, so holding them on it just adds dead air
 * between doing the thing and being told what is next. Long lines actually
 * teach something and need reading time.
 *
 * So: a snappy floor, and per-character time that only starts counting past
 * the length of a bare confirmation. A "Next" button is always there for
 * anyone who wants longer, which is what makes erring on the fast side safe.
 */
const SUCCESS_DWELL_FLOOR_MS = 750
const SUCCESS_DWELL_PER_CHAR_MS = 26
/** Characters covered by the floor before per-character time kicks in. */
const SUCCESS_DWELL_FREE_CHARS = 30

function successDwell(message: string): number {
  const extra = Math.max(0, message.length - SUCCESS_DWELL_FREE_CHARS)
  return SUCCESS_DWELL_FLOOR_MS + extra * SUCCESS_DWELL_PER_CHAR_MS
}

/**
 * Narrowest window where the follow-up note can sit beside a centred modal
 * without touching it: the modal maxes out at 32rem, the note is 18rem, plus
 * margins and a comfortable gap.
 */
const FOLLOW_UP_SIDE_BY_SIDE_MIN_WIDTH = 1180

/**
 * Drives the running tutorial: watches for the gesture each lesson asks for,
 * and advances when it happens.
 *
 * Completion is read from real application state -- the branch actually
 * changed, the sync modal actually opened, a second commit is actually selected
 * -- rather than from handlers bolted onto the elements themselves. That keeps
 * the instrumentation out of the components being taught, and it means a lesson
 * can only be passed by genuinely doing the thing. Clicking around until
 * something happens is, in this case, exactly the intended outcome.
 */
export function TutorialHost() {
  const active = useTutorialStore((s) => s.active)
  const step = useTutorialStore((s) => s.step)
  const successNonce = useTutorialStore((s) => s.successNonce)
  const completeStep = useTutorialStore((s) => s.completeStep)
  const skipStep = useTutorialStore((s) => s.skipStep)
  const exit = useTutorialStore((s) => s.exit)

  const lesson = LESSONS[step]

  // Freezes the lesson on its success line for a beat before moving on, so the
  // user sees their action acknowledged instead of the card jumping.
  const [succeeded, setSucceeded] = useState(false)
  const advancing = useRef(false)

  // True once the gesture landed and the lesson has a follow-up to run. The
  // overlay stands down for this phase so the panel the gesture just opened is
  // readable and usable -- the coach card renders above modals, so leaving it
  // up covered the very thing the lesson had asked the user to open.
  const [inFollowUp, setInFollowUp] = useState(false)

  // Reset the per-lesson latches whenever the step changes.
  useEffect(() => {
    setSucceeded(false)
    setInFollowUp(false)
    advancing.current = false
  }, [step])

  // Bring the target into view before the scrim lands on it, so the spotlight
  // never points at an element that is scrolled out of sight.
  useEffect(() => {
    if (!active || !lesson) return
    // Scroll to what the user has to touch, not to the container holding it:
    // for a lesson that keeps a whole panel live, targets[0] is that panel and
    // scrolling to it says nothing about where the row actually is.
    scrollTutorialTargetIntoView(lesson.ringTargets?.[0]?.[0] ?? lesson.targets[0])
  }, [active, lesson])

  const signal = useTutorialSignal(active ? (lesson?.completeOn ?? null) : null)
  // Only armed during the follow-up, so the closing signal cannot be mistaken
  // for the gesture itself (the sync panel is shut before it is ever opened).
  const followUpSignal = useTutorialSignal(
    active && inFollowUp ? (lesson?.followUp?.until ?? null) : null
  )

  useEffect(() => {
    if (!active || !signal || advancing.current) return
    advancing.current = true
    setSucceeded(true)
  }, [active, signal])

  // Advance (or hand over to the follow-up) once the success line has had its
  // moment.
  //
  // Split out of the effect above on purpose. Scheduling the timer there meant
  // a teardown -- which `setSucceeded` itself triggers on the very next render
  // -- cleared the timeout, and the re-run then hit the `advancing` guard and
  // returned without rescheduling it. The timer was cancelled and never
  // replaced, so the drag lesson sat on its success line forever and the
  // follow-up never started. Keying this effect on `succeeded` means it is
  // scheduled exactly once, when that flag flips, with nothing to cancel it.
  useEffect(() => {
    if (!succeeded || inFollowUp) return
    const dwell = successDwell(lesson?.success ?? '')
    const timer = window.setTimeout(() => {
      if (lesson?.followUp) setInFollowUp(true)
      else completeStep(LESSONS.length)
    }, dwell)
    return () => window.clearTimeout(timer)
  }, [succeeded, inFollowUp, lesson, completeStep])

  // The follow-up ends when its own signal arrives (the panel was closed).
  useEffect(() => {
    if (!inFollowUp || !followUpSignal) return
    completeStep(LESSONS.length)
  }, [inFollowUp, followUpSignal, completeStep])

  // Escape is the universal "let me out" and costs nothing to support.
  useEffect(() => {
    if (!active) return
    const onKeyDown = (e: KeyboardEvent) => {
      // During a follow-up, Escape belongs to the panel the user is reading --
      // it is how they close it, which is also how the lesson finishes. Taking
      // it here would quit the whole tour instead.
      if (e.key === 'Escape' && !inFollowUp) {
        e.preventDefault()
        exit()
      }
    }
    window.addEventListener('keydown', onKeyDown, true)
    return () => window.removeEventListener('keydown', onKeyDown, true)
  }, [active, exit, inFollowUp])

  if (!active || !lesson) return null

  // Follow-up phase: no scrim, no rings, no card over the panel. Just a note
  // in the corner explaining what the user is looking at, while they use the
  // real thing. The lesson completes when they close it.
  if (inFollowUp && lesson.followUp) {
    return <FollowUpNote followUp={lesson.followUp} step={step} total={LESSONS.length} />
  }

  return (
    <TutorialOverlay
      targetIds={lesson.targets}
      ringTargetIds={lesson.ringTargets}
      dragFromIndex={lesson.dragFromIndex}
      dropTargetIndex={lesson.dropTargetIndex}
      secondaryDragFromIndex={lesson.secondaryDragFromIndex}
      secondaryDropTargetIndex={lesson.secondaryDropTargetIndex}
      gesture={lesson.gesture}
      successNonce={successNonce}
    >
      {(rect) => (
        <TutorialCoach
          lesson={lesson}
          rect={rect}
          dock={lesson.dock}
          step={step}
          total={LESSONS.length}
          succeeded={succeeded}
          onSkip={() => skipStep(LESSONS.length)}
          // Advancing from the success line goes to the follow-up when the
          // lesson has one, rather than skipping the panel the user just opened.
          onAdvance={() =>
            lesson.followUp ? setInFollowUp(true) : completeStep(LESSONS.length)
          }
          onExit={exit}
        />
      )}
    </TutorialOverlay>
  )
}

/**
 * The note shown while the user reads a panel their gesture just opened.
 *
 * Docked against the right edge, vertically centred: the panel it describes is
 * a centred modal, so this is the one place the two can both be read without
 * either covering the other. Deliberately narrow for the same reason. Nothing
 * here is modal -- the user is meant to be clicking around the real thing while
 * it is up.
 */
function FollowUpNote({
  followUp,
  step,
  total,
}: {
  followUp: NonNullable<Lesson['followUp']>
  step: number
  total: number
}) {
  // Side by side needs room for the modal (32rem, centred) plus this card.
  // On a narrow window they would overlap -- which is the very problem this
  // phase exists to solve -- so there it drops to the bottom-left instead,
  // below a modal that is never tall enough to reach the floor.
  const [sideBySide, setSideBySide] = useState(
    () => window.innerWidth >= FOLLOW_UP_SIDE_BY_SIDE_MIN_WIDTH
  )
  useEffect(() => {
    const onResize = () =>
      setSideBySide(window.innerWidth >= FOLLOW_UP_SIDE_BY_SIDE_MIN_WIDTH)
    window.addEventListener('resize', onResize)
    return () => window.removeEventListener('resize', onResize)
  }, [])

  return createPortal(
    <div className="pointer-events-none fixed inset-0 z-[200]">
      <div
        className={cn(
          'pointer-events-auto absolute w-72 rounded-xl border border-border bg-panel p-4 shadow-2xl',
          sideBySide ? 'right-5 top-1/2 -translate-y-1/2' : 'bottom-5 left-5'
        )}
      >
        <span className="font-mono text-2xs uppercase tracking-wide text-muted-foreground">
          Step {step + 1} of {total}
        </span>
        <h2 className="mt-1.5 text-sm font-semibold text-foreground">{followUp.title}</h2>
        <p className="mt-1.5 text-[0.78125rem] leading-relaxed text-sub">{followUp.body}</p>
        <div className="mt-3 flex items-center gap-2 rounded-lg border border-border bg-background px-2.5 py-2">
          <Hand size={13} className="mt-px flex-none text-accent-text" />
          <span className="text-2xs leading-relaxed text-foreground">
            Close the panel when you are done reading.
          </span>
        </div>
      </div>
    </div>,
    document.body
  )
}

/**
 * True once the gesture a lesson is waiting for has happened.
 *
 * Each signal is derived from state the app already keeps, and is measured
 * against a baseline captured when the lesson starts -- otherwise a lesson
 * would count a condition that was already true before the user did anything
 * (the sync modal left open from the previous step, a branch that was already
 * checked out) and skip itself instantly.
 */
function useTutorialSignal(kind: TutorialSignal | null): boolean {
  const repo = useActiveRepo()
  const activeModal = useUiStore((s) => s.activeModal)
  const selectedShas = useUiStore((s) => s.selectedShas)

  // Read the checked-out branch from the branch list, not from
  // `repo.head_branch`. That field is captured once when the repo is opened and
  // is never rewritten on checkout, so it stays frozen at whatever was current
  // at open time -- the switch lesson could never see a change and sat there
  // forever even though the branch really had switched. The branches query is
  // invalidated by the checkout mutation, so it reflects reality.
  const branches = useBranches(repo?.id ?? null)
  const head = branches.data?.local.find((b) => b.is_head)?.name ?? null

  const [fired, setFired] = useState(false)
  const baseline = useRef<{ head: string | null } | null>(null)

  // Clear the baseline when the lesson changes; the effect below fills it in
  // once there is a real value to record.
  useEffect(() => {
    setFired(false)
    baseline.current = null
  }, [kind])

  // Capture the starting branch on the first render that actually has one.
  // The branch list arrives asynchronously, so recording the baseline eagerly
  // would store `null` and then read the query resolving as "the branch
  // changed", passing the lesson before the user touched anything.
  useEffect(() => {
    if (kind === 'branch-switched' && baseline.current === null && head !== null) {
      baseline.current = { head }
    }
  }, [kind, head])

  useEffect(() => {
    if (!kind || fired) return
    const base = baseline.current
    let hit = false

    switch (kind) {
      case 'branch-switched':
        // Both sides non-null: a baseline that never got recorded, or a branch
        // list that has not loaded, must not count as a switch.
        hit = base?.head != null && head != null && head !== base.head
        break
      case 'sync-opened':
        hit = activeModal === 'remote-sync'
        break
      case 'sync-closed':
        // Only armed once the follow-up starts, so "not open" here always means
        // the user closed a panel that was open a moment ago.
        hit = activeModal !== 'remote-sync'
        break
      case 'multi-selected':
        hit = selectedShas.length > 1
        break
      case 'context-menu-opened':
        // Radix marks the trigger it opened from; simplest honest read of "a
        // context menu is on screen" without wiring into every menu.
        hit = !!document.querySelector('[data-radix-menu-content]')
        break
    }

    if (hit) setFired(true)
  }, [kind, fired, head, activeModal, selectedShas])

  // The context-menu signal is the one thing not backed by a store, so it needs
  // a poll to notice the menu appearing. Cheap, and only while that lesson runs.
  useEffect(() => {
    if (kind !== 'context-menu-opened' || fired) return
    const timer = window.setInterval(() => {
      if (document.querySelector('[data-radix-menu-content]')) setFired(true)
    }, 150)
    return () => window.clearInterval(timer)
  }, [kind, fired])

  return fired
}
