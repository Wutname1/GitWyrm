import { useState } from 'react'
import { Send, ShieldCheck, Square } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { cn } from '@/lib/utils'
import type { AiRun } from '@/hooks/useAiRun'
import { isActive, stateGlyph, stateLabel } from '@/hooks/useAiRun'
import { RunEndingCard, type EndingActions } from './RunEndingCard'
import { RunFinishedCard } from './RunFinishedCard'
import { RunStream } from './RunStream'

/**
 * The ✦ AI tab: one run, start to finish.
 *
 * Everything here describes the run named in the header. The header is not
 * allowed to say one task while the stream shows another, which is why the
 * session carries its own change id rather than reading whatever is selected.
 */
export function AiRunTab({
  run,
  onViewDiff,
  endingActions,
  provider,
}: {
  run: AiRun
  onViewDiff?: (path: string) => void
  endingActions: EndingActions
  /**
   * Display name of the AI that did the work, for the `Assisted-by` trailer.
   * Absent when it cannot be determined, in which case the run falls back to the
   * plain ending rather than drafting a commit that would misattribute itself.
   */
  provider?: string
}) {
  const [draft, setDraft] = useState('')
  const [busy, setBusy] = useState(false)

  if (!run.session || !run.state) {
    return (
      <div className="p-6 text-center text-2xs text-muted-foreground">
        No task is running. Start one from a task in the list.
      </div>
    )
  }

  const { session, state } = run
  const active = isActive(state)
  const ended = run.steps.find((s) => s.kind === 'ended')

  const send = async () => {
    const text = draft.trim()
    if (!text) return
    setDraft('')
    await run.note(text)
  }

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex-none border-b border-border px-3 py-2">
        <div className="flex items-center gap-2">
          <div className="min-w-0 flex-1">
            <div className="truncate text-xs font-semibold text-foreground">
              Task {session.task_number} · {session.task_text}
            </div>
          </div>
          <StatePill state={state} />
          {active && (
            <Button
              variant="ghost"
              size="sm"
              className="h-7 flex-none px-2 text-xs text-muted-foreground hover:text-destructive"
              disabled={busy}
              onClick={async () => {
                setBusy(true)
                await run.stop()
                setBusy(false)
              }}
            >
              <Square size={11} />
              Stop
            </Button>
          )}
        </div>

        <div className="mt-1.5 flex items-start gap-1.5 text-2xs text-muted-foreground">
          <ShieldCheck size={11} className="mt-0.5 flex-none" />
          <span>
            Working on <span className="text-sub">{session.branch}</span>
            {/* Name the folder whenever it is not the user's own. A run that
                says "working on branch X" while editing files the user cannot
                see would be claiming to touch the files in front of them. */}
            {session.worktree_name && (
              <>
                {' '}
                in its own <span className="text-sub">{session.worktree_name}</span> folder, so
                your files stay untouched while it works
              </>
            )}
            . One commit per task, never pushes, and you can undo everything it does.
          </span>
        </div>
      </div>

      <RunStream
        steps={run.steps}
        onAnswerGate={async (answer) => {
          setBusy(true)
          await run.answerGate(answer)
          setBusy(false)
        }}
        onViewDiff={onViewDiff}
        gateBusy={busy}
      />

      {/* A run that finished its task gets the review-and-commit card; one that
          stopped or failed gets the plain ending, because there is nothing to
          approve. Both say the same thing first: nothing was committed. */}
      {ended?.kind === 'ended' && !active && ended.state === 'finished' && provider && (
        <div className="flex-none border-t border-border p-2">
          <RunFinishedCard
            repoId={session.repo_id}
            changeId={session.change_id}
            taskNumber={session.task_number}
            provider={provider}
            onCommitted={endingActions.onCommitted ?? (() => {})}
            onUndo={endingActions.onUndo}
          />
        </div>
      )}

      {ended?.kind === 'ended' && !active && (ended.state !== 'finished' || !provider) && (
        <div className="flex-none border-t border-border p-2">
          <RunEndingCard
            state={ended.state}
            detail={ended.detail}
            actions={endingActions}
            needsReconnect={ended.detail.toLowerCase().includes('sign-in')}
          />
        </div>
      )}

      {active && (
        <div className="flex-none border-t border-border p-2">
          <div className="flex items-center gap-1.5">
            <Input
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter' && !e.shiftKey) {
                  e.preventDefault()
                  void send()
                }
              }}
              placeholder="Add a note - it waits until the current step finishes"
              className="h-7 flex-1 text-xs"
            />
            <Button
              size="sm"
              className="h-7 flex-none px-2 text-xs"
              disabled={!draft.trim()}
              onClick={() => void send()}
            >
              <Send size={11} />
            </Button>
          </div>
          <button
            type="button"
            className="mt-1.5 rounded px-1.5 py-0.5 text-2xs text-muted-foreground hover:bg-panel2 hover:text-foreground"
            onClick={() => void run.note('Explain what you are doing and why.')}
          >
            Explain what you're doing
          </button>
        </div>
      )}
    </div>
  )
}

/**
 * The state pill.
 *
 * The glyph is load-bearing: "Needs you" sits in the same window as a change's
 * "Needs review", and colour alone does not separate them for someone
 * scanning rather than reading.
 */
export function StatePill({ state }: { state: RunStateName }) {
  return (
    <span
      className={cn(
        'flex-none rounded-full px-2 py-0.5 text-2xs font-semibold',
        state === 'needsYou' && 'bg-amber-500/15 text-amber-500',
        (state === 'working' || state === 'preparing') && 'bg-soft text-accent-text',
        state === 'finished' && 'bg-green-500/15 text-green-500',
        state === 'failed' && 'bg-destructive/15 text-destructive',
        state === 'stopped' && 'bg-panel2 text-muted-foreground'
      )}
    >
      {stateGlyph(state)} {stateLabel(state)}
    </span>
  )
}

type RunStateName = Parameters<typeof stateGlyph>[0]
