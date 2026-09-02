import { useEffect, useRef, useState } from 'react'
import { listen } from '@tauri-apps/api/event'
import { Sparkles } from 'lucide-react'
import { PendingIndicator } from '@/components/ui/pending-indicator'

interface AiResolveProgress {
  path: string
  kind: string
  text: string
}

/**
 * What the model is doing, while it is still doing it.
 *
 * A resolve can take a couple of minutes, and a spinner that long is
 * indistinguishable from a hang. The model's reasoning is streamed here as it
 * arrives so the wait shows its work instead of just its length.
 *
 * Only the reasoning is shown, never the answer text: the answer lands in the
 * editor for review the moment it is complete, and printing it twice would
 * invite reading it here -- where it cannot be edited and is not yet checked
 * for leftover conflict markers.
 */
export function AiResolveStream({ path, running }: { path: string | null; running: boolean }) {
  const [thinking, setThinking] = useState('')
  const [started, setStarted] = useState(false)
  const [elapsed, setElapsed] = useState(0)
  const bodyRef = useRef<HTMLDivElement>(null)

  // Clear on a new run rather than when it ends, so the reasoning stays
  // readable after the draft lands -- that is when it is worth reading.
  useEffect(() => {
    if (!running) return
    setThinking('')
    setStarted(false)
    setElapsed(0)
  }, [running, path])

  useEffect(() => {
    if (!path) return
    const unlisten = listen<AiResolveProgress>('ai-resolve-progress', (event) => {
      // Events are scoped by file so switching mid-run cannot mix two streams.
      if (event.payload.path !== path) return
      if (event.payload.kind === 'starting') {
        setStarted(true)
        return
      }
      if (event.payload.kind === 'thinking') {
        setThinking((current) => current + event.payload.text)
      }
    })
    return () => {
      unlisten.then((dispose) => dispose())
    }
  }, [path])

  useEffect(() => {
    if (!running) return
    const tick = window.setInterval(() => setElapsed((s) => s + 1), 1000)
    return () => window.clearInterval(tick)
  }, [running])

  // Follow the newest text, the way a log pane does.
  useEffect(() => {
    const el = bodyRef.current
    if (el) el.scrollTop = el.scrollHeight
  }, [thinking])

  if (!running && !thinking) return null

  return (
    <div className="flex-none border-b border-border bg-panel2">
      <div className="flex items-center gap-2 px-3 py-1.5">
        {running ? <PendingIndicator /> : <Sparkles size={12} className="text-accent-text" />}
        <span className="text-2xs font-bold tracking-[.05em] text-sub">
          {running ? 'AI IS WORKING' : 'WHAT THE AI WAS THINKING'}
        </span>
        {running && (
          <span className="ml-auto text-2xs text-muted-foreground">
            {elapsed}s
          </span>
        )}
      </div>
      <div
        ref={bodyRef}
        className="max-h-28 overflow-y-auto px-3 pb-2 font-mono text-2xs leading-relaxed text-muted-foreground"
      >
        {thinking ? (
          <span className="whitespace-pre-wrap break-words">{thinking}</span>
        ) : (
          <span className="italic">
            {started
              ? 'Reading the conflict…'
              : 'Starting the model… (the first run also starts the assistant, which is slower)'}
          </span>
        )}
      </div>
    </div>
  )
}
