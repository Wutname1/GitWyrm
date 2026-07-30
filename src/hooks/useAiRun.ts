import { useCallback, useEffect } from 'react'
import { listen } from '@tauri-apps/api/event'
import { useAiRunStore, EMPTY_RUN } from '@/stores/aiRunStore'
import {
  commands,
  type DemoScenario,
  type GateAnswer,
  type RunEventKind,
  type RunSession,
  type RunState,
  type RunStep,
} from '@/lib/bindings'

/**
 * The run the console shows, and the steps it has produced.
 *
 * Listens to a global app event rather than holding a channel, so a gate that
 * opens while the user is on another tab -- or in the other window -- still
 * reaches them. Every surface that mirrors a run reads from this same stream.
 */
export interface AiRun {
  session: RunSession | null
  steps: RunStep[]
  state: RunState | null
  /** The most recent sentence, for the status bar and the spec card. */
  latest: string
  /** The gate waiting on an answer, if any. */
  openGate: Extract<RunStep, { kind: 'gate' }> | null
  answerGate: (answer: GateAnswer) => Promise<void>
  note: (text: string) => Promise<void>
  stop: () => Promise<void>
  clear: () => Promise<void>
  startDemo: (opts: DemoStart) => Promise<string | null>
}

export interface DemoStart {
  changeId: string
  taskNumber: number
  taskText: string
  branch: string
  scenario: DemoScenario
}

/** The gate is open only when it is the newest step -- answering moves it on. */
function findOpenGate(
  steps: RunStep[],
  state: RunState | null
): Extract<RunStep, { kind: 'gate' }> | null {
  if (state !== 'needsYou') return null
  const last = steps[steps.length - 1]
  return last?.kind === 'gate' ? last : null
}

/**
 * Subscribes to the app-wide run event once, for the whole window.
 *
 * Mounted from the root rather than per component: five surfaces render a run,
 * and one listener per surface would apply every event five times.
 */
export function useAiRunListener() {
  const applyEvent = useAiRunStore((s) => s.applyEvent)
  useEffect(() => {
    const unlisten = listen<RunEventKind>('ai-run-event', (event) => {
      applyEvent(event.payload)
    })
    return () => {
      void unlisten.then((fn) => fn())
    }
  }, [applyEvent])
}

export function useAiRun(repoId: string | null): AiRun {
  // Read from the shared store, so every surface shows the same run. Component
  // state here would give each caller its own copy and leave the mirrors blank.
  const entry = useAiRunStore((s) => (repoId ? s.byRepo[repoId] : undefined)) ?? EMPTY_RUN
  const setRun = useAiRunStore((s) => s.setRun)
  const clearRun = useAiRunStore((s) => s.clearRun)
  const { session, steps, state, latest } = entry

  useEffect(() => {
    if (!repoId) return
    let cancelled = false
    void (async () => {
      const res = await commands.aiRunCurrent(repoId)
      if (cancelled || res.status !== 'ok') return
      const current = res.data
      // Only seed from the backend when this window has nothing yet. Otherwise
      // a late load would wipe steps that arrived while it was in flight.
      if (useAiRunStore.getState().byRepo[repoId]) return
      setRun(repoId, {
        session: current,
        steps: current?.steps ?? [],
        state: current?.state ?? null,
        latest: '',
      })
    })()
    return () => {
      cancelled = true
    }
  }, [repoId, setRun])

  const answerGate = useCallback(
    async (answer: GateAnswer) => {
      if (!repoId || !session) return
      await commands.aiRunAnswerGate(repoId, session.session_id, answer)
    },
    [repoId, session]
  )

  const note = useCallback(
    async (text: string) => {
      if (!repoId || !session || !text.trim()) return
      await commands.aiRunNote(repoId, session.session_id, text.trim())
    },
    [repoId, session]
  )

  const stop = useCallback(async () => {
    if (!repoId || !session) return
    await commands.aiRunStop(repoId, session.session_id)
  }, [repoId, session])

  const clear = useCallback(async () => {
    if (!repoId) return
    await commands.aiRunClear(repoId)
    clearRun(repoId)
  }, [repoId, clearRun])

  const startDemo = useCallback(
    async (opts: DemoStart): Promise<string | null> => {
      if (!repoId) return null
      const res = await commands.aiRunStartDemo(
        repoId,
        opts.changeId,
        opts.taskNumber,
        opts.taskText,
        opts.branch,
        opts.scenario
      )
      if (res.status !== 'ok') return 'That could not be started.'
      if (res.data.kind === 'alreadyRunning') return res.data.summary
      setRun(repoId, {
        session: res.data.session,
        steps: [],
        state: res.data.session.state,
        latest: '',
      })
      return null
    },
    [repoId, setRun]
  )

  return {
    session,
    steps,
    state,
    latest,
    openGate: findOpenGate(steps, state),
    answerGate,
    note,
    stop,
    clear,
    startDemo,
  }
}

/** The glyph the state pill shows. Mirrors the backend's own mapping. */
export function stateGlyph(state: RunState): string {
  switch (state) {
    case 'preparing':
    case 'working':
      return '●'
    case 'needsYou':
      return '⏸'
    case 'finished':
      return '✓'
    case 'stopped':
      return '■'
    case 'failed':
      return '✕'
  }
}

/**
 * The pill's label.
 *
 * "Needs you" sits in the same window as a change's "Needs review" status, so
 * the glyph in front of it is doing real work -- colour alone would not
 * separate them for someone scanning.
 */
export function stateLabel(state: RunState): string {
  switch (state) {
    case 'preparing':
      return 'Getting ready'
    case 'working':
      return 'Working'
    case 'needsYou':
      return 'Needs you'
    case 'finished':
      return 'Finished'
    case 'stopped':
      return 'Stopped'
    case 'failed':
      return "Didn't finish"
  }
}

/** Whether Stop should be offered. */
export function isActive(state: RunState | null): boolean {
  return state === 'preparing' || state === 'working' || state === 'needsYou'
}
