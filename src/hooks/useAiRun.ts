import { useCallback, useEffect, useRef, useState } from 'react'
import { listen } from '@tauri-apps/api/event'
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

export function useAiRun(repoId: string | null): AiRun {
  const [session, setSession] = useState<RunSession | null>(null)
  const [steps, setSteps] = useState<RunStep[]>([])
  const [state, setState] = useState<RunState | null>(null)
  const [latest, setLatest] = useState('')

  // Held in a ref as well as state so the event handler can compare without
  // re-subscribing on every event.
  const sessionIdRef = useRef<string | null>(null)

  const load = useCallback(async () => {
    if (!repoId) {
      setSession(null)
      setSteps([])
      setState(null)
      sessionIdRef.current = null
      return
    }
    const res = await commands.aiRunCurrent(repoId)
    if (res.status !== 'ok') return
    const current = res.data
    setSession(current)
    sessionIdRef.current = current?.session_id ?? null
    setSteps(current?.steps ?? [])
    setState(current?.state ?? null)
  }, [repoId])

  useEffect(() => {
    void load()
  }, [load])

  useEffect(() => {
    if (!repoId) return
    const unlisten = listen<RunEventKind>('ai-run-event', (event) => {
      const e = event.payload
      if (e.repo_id !== repoId) return
      // A run that was replaced must not write into the newer one's console.
      // The backend drops these too; this is the same rule on the near side,
      // so a late event cannot slip in between a start and a reload.
      if (sessionIdRef.current && e.session_id !== sessionIdRef.current) return
      setState(e.state)
      setLatest(e.summary)
      setSteps((prev) => [...prev, e.step])
    })
    return () => {
      void unlisten.then((fn) => fn())
    }
  }, [repoId])

  const answerGate = useCallback(
    async (answer: GateAnswer) => {
      if (!repoId || !sessionIdRef.current) return
      await commands.aiRunAnswerGate(repoId, sessionIdRef.current, answer)
    },
    [repoId]
  )

  const note = useCallback(
    async (text: string) => {
      if (!repoId || !sessionIdRef.current || !text.trim()) return
      await commands.aiRunNote(repoId, sessionIdRef.current, text.trim())
    },
    [repoId]
  )

  const stop = useCallback(async () => {
    if (!repoId || !sessionIdRef.current) return
    await commands.aiRunStop(repoId, sessionIdRef.current)
  }, [repoId])

  const clear = useCallback(async () => {
    if (!repoId) return
    await commands.aiRunClear(repoId)
    setSession(null)
    setSteps([])
    setState(null)
    setLatest('')
    sessionIdRef.current = null
  }, [repoId])

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
      setSession(res.data.session)
      sessionIdRef.current = res.data.session.session_id
      setSteps([])
      setState(res.data.session.state)
      return null
    },
    [repoId]
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
