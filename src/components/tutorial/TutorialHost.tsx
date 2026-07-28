import { useEffect, useRef, useState } from 'react'
import { LESSONS, type TutorialSignal } from '@/lib/tutorialLessons'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo } from '@/stores/workspaceStore'
import { useTutorialStore } from '@/stores/tutorialStore'
import { TutorialOverlay } from './TutorialOverlay'
import { TutorialCoach } from './TutorialCoach'
import { scrollTutorialTargetIntoView } from './useTutorialTarget'

/** How long the success line stays up before the next lesson takes over. */
const SUCCESS_DWELL_MS = 1100

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

  // Reset the per-lesson latch whenever the step changes.
  useEffect(() => {
    setSucceeded(false)
    advancing.current = false
  }, [step])

  // Bring the target into view before the scrim lands on it, so the spotlight
  // never points at an element that is scrolled out of sight.
  useEffect(() => {
    if (!active || !lesson) return
    scrollTutorialTargetIntoView(lesson.targets[0])
  }, [active, lesson])

  const signal = useTutorialSignal(active ? (lesson?.completeOn ?? null) : null)

  useEffect(() => {
    if (!active || !signal || advancing.current) return
    advancing.current = true
    setSucceeded(true)
    const timer = window.setTimeout(() => completeStep(LESSONS.length), SUCCESS_DWELL_MS)
    return () => window.clearTimeout(timer)
  }, [active, signal, completeStep])

  // Escape is the universal "let me out" and costs nothing to support.
  useEffect(() => {
    if (!active) return
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault()
        exit()
      }
    }
    window.addEventListener('keydown', onKeyDown, true)
    return () => window.removeEventListener('keydown', onKeyDown, true)
  }, [active, exit])

  if (!active || !lesson) return null

  return (
    <TutorialOverlay
      targetIds={lesson.targets}
      gesture={lesson.gesture}
      successNonce={successNonce}
    >
      {(rect) => (
        <TutorialCoach
          lesson={lesson}
          rect={rect}
          step={step}
          total={LESSONS.length}
          succeeded={succeeded}
          onSkip={() => skipStep(LESSONS.length)}
          onExit={exit}
        />
      )}
    </TutorialOverlay>
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
  const centerView = useUiStore((s) => s.centerView)
  const head = repo?.head_branch ?? null

  const [fired, setFired] = useState(false)
  const baseline = useRef<{ head: string | null } | null>(null)

  useEffect(() => {
    setFired(false)
    baseline.current = { head }
    // Baseline is captured once per lesson: re-capturing on every head change
    // would move the goalposts to wherever the user just arrived.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [kind])

  useEffect(() => {
    if (!kind || fired) return
    const base = baseline.current
    let hit = false

    switch (kind) {
      case 'branch-switched':
        hit = !!base && head !== base.head
        break
      case 'sync-opened':
        hit = activeModal === 'remote-sync'
        break
      case 'multi-selected':
        hit = selectedShas.length > 1
        break
      case 'context-menu-opened':
        // Radix marks the trigger it opened from; simplest honest read of "a
        // context menu is on screen" without wiring into every menu.
        hit = !!document.querySelector('[data-radix-menu-content]')
        break
      case 'lines-selected':
        hit = centerView === 'diff'
        break
    }

    if (hit) setFired(true)
  }, [kind, fired, head, activeModal, selectedShas, centerView])

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
