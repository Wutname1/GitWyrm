import { useEffect, useState } from 'react'
import {
  Archive,
  CheckCircle2,
  ClipboardCopy,
  Code2,
  MessageCircleQuestion,
  Play,
  SquareTerminal,
} from 'lucide-react'
import { toast } from 'sonner'
import { cn } from '@/lib/utils'
import type { CliOutcome, DemoScenario, SpecChange } from '@/lib/bindings'
import { commands } from '@/lib/bindings'
import { unwrap } from '@/lib/queryKeys'
import { composeTaskHandoff, copyTaskHandoff } from '@/lib/specHandoff'
import { nextTask, useOpenspecMutations } from '@/hooks/useOpenspec'
import { useSpecAi } from '@/hooks/useSpecAi'
import { stateGlyph, stateLabel, useAiRun } from '@/hooks/useAiRun'
import { useBranches } from '@/hooks/useGitQueries'
import { ConfirmDialog } from '@/components/modals/ConfirmDialog'
import { describeError, log } from '@/lib/log'

/** A button in the rail. Primary is the one action the state calls for. */
function RailButton({
  icon,
  label,
  onClick,
  primary,
  pending,
  disabled,
  title,
}: {
  icon?: React.ReactNode
  label: string
  onClick: () => void
  primary?: boolean
  pending?: boolean
  disabled?: boolean
  /** Shown on hover, so a disabled button can say why. */
  title?: string
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      title={title}
      disabled={pending || disabled}
      className={cn(
        'flex min-h-8 w-full items-center justify-center gap-2 rounded-md px-3 text-2xs font-semibold transition-colors disabled:opacity-50',
        primary
          ? 'bg-primary text-primary-foreground hover:brightness-110'
          : 'border border-border bg-panel2 text-foreground hover:border-muted-foreground hover:bg-panel3'
      )}
    >
      {icon}
      {pending ? 'Working…' : label}
    </button>
  )
}

/**
 * The inline result of a spec check.
 *
 * Deliberately not a toast: a toast is gone in four seconds, and the whole point
 * of a check is to sit there while the user fixes what it found. It stays until
 * the change is switched or the check is run again.
 */
function CheckResult({ outcome }: { outcome: CliOutcome }) {
  if (outcome.kind === 'cliMissing') {
    return (
      <div className="mt-2 rounded-md border border-[var(--gw-amber)]/40 bg-[var(--gw-amber)]/8 px-3 py-2 text-2xs leading-relaxed text-[var(--gw-amber)]">
        <p className="font-semibold">The OpenSpec tool is not installed.</p>
        <p className="mt-1 text-[var(--gw-amber)]/85">{outcome.hint}</p>
      </div>
    )
  }
  if (outcome.kind === 'failed') {
    return (
      <div className="mt-2 rounded-md border border-[var(--gw-red)]/40 bg-[var(--gw-red)]/8 px-3 py-2 text-2xs leading-relaxed">
        <p className="font-semibold text-removed">This change has problems to fix.</p>
        {outcome.output.trim() && (
          <pre className="mt-1.5 max-h-40 overflow-auto whitespace-pre-wrap font-mono text-2xs text-sub">
            {outcome.output.trim()}
          </pre>
        )}
      </div>
    )
  }
  return (
    <div className="mt-2 rounded-md border border-primary/40 bg-soft px-3 py-2 text-2xs leading-relaxed">
      <p className="font-semibold text-accent-text">The spec check passed.</p>
      <p className="mt-1 text-sub">
        The proposal, tasks, and requirements are all shaped the way OpenSpec expects.
      </p>
      {outcome.output.trim() && (
        <pre className="mt-1.5 max-h-40 overflow-auto whitespace-pre-wrap font-mono text-2xs text-muted-foreground">
          {outcome.output.trim()}
        </pre>
      )}
    </div>
  )
}

/**
 * The Desk's right column: hand this change's next task to whatever tool the
 * user prefers, check it, and archive it when it is done.
 *
 * Everything here works with no AI configured -- copy a handoff and paste it
 * anywhere. `add-ai-provider-surface` later reorders this card to put an in-app
 * run first, and moves these into a disclosure; nothing here is replaced.
 */
export function DeskActionRail({
  change,
  repoId,
  repoPath,
}: {
  change: SpecChange
  repoId: string
  repoPath: string
}) {
  const ai = useSpecAi()
  const { validateChange, archiveChange } = useOpenspecMutations(repoId)
  const [result, setResult] = useState<CliOutcome | null>(null)
  const [starting, setStarting] = useState(false)
  const branches = useBranches(repoId)
  const [confirmArchive, setConfirmArchive] = useState(false)
  // Held here, not inside the <details>: the rail re-renders on every task tick
  // and watcher refresh, and a details element rebuilt from JSX would snap shut
  // under the user mid-read.
  const [handoffOpen, setHandoffOpen] = useState(false)
  const [inviteDismissed, setInviteDismissed] = useState(false)

  const task = nextTask(change)
  const allDone = !task && !change.progress.is_draft
  const remaining = change.progress.total - change.progress.done
  const handoff = composeTaskHandoff(change, task)

  // A result belongs to the change it was run on. Switching changes has to clear
  // it, or the rail would show one change's problems beside another's name.
  useEffect(() => {
    setResult(null)
  }, [change.id])

  const runCheck = () => {
    validateChange.mutate(change.id, {
      onSuccess: setResult,
      onError: (e) => toast.error(describeError(e)),
    })
  }

  const archive = () => {
    archiveChange.mutate(change.id, {
      onSuccess: (outcome) => {
        if (outcome.kind === 'ok') {
          toast.success(`Archived ${change.id}.`, {
            description: 'Its requirements are part of your specs now.',
          })
          setConfirmArchive(false)
          return
        }
        // Keep the dialog open on failure: the change did not move, and closing
        // would imply it did.
        setResult(outcome)
        toast.error(`Could not archive ${change.id}.`)
      },
      onError: (e) => toast.error(describeError(e)),
    })
  }

  const openInOpencode = async () => {
    // Clipboard first: the terminal opens whether or not opencode is installed,
    // and the handoff is the part the user actually needs in hand.
    await copyTaskHandoff(change, task, { silent: true })
    try {
      unwrap(await commands.openInTerminal(repoId))
      toast.success('Terminal opened with the handoff copied.', {
        description: 'Start opencode and paste it.',
      })
    } catch (e) {
      log.error(`spec desk: open in terminal failed: ${describeError(e)}`)
      toast.error('Could not open a terminal here.', { description: describeError(e) })
    }
  }

  const openInVsCode = async () => {
    await copyTaskHandoff(change, task, { silent: true })
    try {
      unwrap(await commands.openInEditor(repoId, 'vs_code'))
      toast.success('VS Code opened with the handoff copied.')
    } catch (e) {
      log.error(`spec desk: open in editor failed: ${describeError(e)}`)
      toast.error('Could not open VS Code.', { description: describeError(e) })
    }
  }

  const startRun = async () => {
    if (!task || starting) return
    setStarting(true)
    try {
      const res = await commands.aiRunStart(
        repoId,
        change.id,
        task.index,
        // The author's own numbering is what the header shows; the index is
        // what identifies the checkbox. They disagree whenever a plan skips or
        // repeats a number, so both are sent rather than one being derived.
        change.progress.done + 1,
        task.text,
        // The checked-out branch: a run edits files in place, so it has to be
        // pinned to where the work will actually land.
        branches.data?.local.find((b) => b.is_head)?.name ?? 'main'
      )
      if (res.status !== 'ok') {
        toast.error('That could not be started.', { description: res.error })
        return
      }
      if (res.data.kind === 'alreadyRunning') {
        toast.info(res.data.summary)
        return
      }
      toast.success('Started. Watch it on the AI tab.')
    } finally {
      setStarting(false)
    }
  }

  return (
    <div className="flex min-h-0 flex-col border-l border-border bg-panel">
      <header className="flex-none border-b border-border px-4 py-3.5">
        <h2 className="text-sm font-semibold text-foreground">Next best action</h2>
        <p className="mt-0.5 text-2xs text-muted-foreground">
          Work from the spec. Keep the tools you already use.
        </p>
      </header>

      <div className="min-h-0 flex-1 overflow-y-auto p-3.5">
        <RunBanner repoId={repoId} />
        <section className="rounded-lg border border-primary/25 bg-soft px-3.5 py-3">
          <p className="text-2xs font-bold tracking-[.09em] text-accent-text">
            {task
              ? `NEXT TASK · ${change.progress.done + 1} OF ${change.progress.total}`
              : allDone
                ? 'EVERY TASK IS DONE'
                : 'NO TASKS YET'}
          </p>
          <h3 className="mt-1.5 text-xs font-semibold leading-snug text-foreground">
            {task
              ? task.text
              : allDone
                ? 'Check it, then archive it'
                : 'Add tasks to tasks.md to get started'}
          </h3>
          <p className="mt-1.5 text-2xs leading-relaxed text-sub">
            {ai.configured
              ? `${ai.providerShort} reads this change's proposal and agreed behavior first, then does just this task. You watch every step and can stop anytime.`
              : "The handoff carries this change's proposal, its agreed behavior, and the one task - so whatever you paste it into does not have to guess."}
          </p>

          <div className="mt-3 flex flex-col gap-2">
            {ai.configured ? (
              <>
                {task && (
                  <RailButton
                    primary
                    icon={<Play size={12} strokeWidth={2.6} />}
                    label={starting ? 'Starting…' : 'Run this task with AI'}
                    onClick={() => void startRun()}
                  />
                )}
                {/* Asking questions in-app is its own change (`add-ai-ask-mode`).
                    Until it lands the button is disabled with a tooltip rather
                    than clickable-then-apologetic: a control that does nothing
                    but explain itself trains people to stop trusting buttons. */}
                <RailButton
                  icon={<MessageCircleQuestion size={12} strokeWidth={2.2} />}
                  label="Ask about this change"
                  disabled
                  title="Not built yet - copy the handoff below and ask in your own tool."
                  onClick={() => {}}
                />
              </>
            ) : (
              <>
                <RailButton
                  primary
                  icon={<ClipboardCopy size={12} strokeWidth={2.4} />}
                  label={allDone ? 'Copy review handoff' : 'Copy task handoff'}
                  onClick={() => void copyTaskHandoff(change, task)}
                />
                <RailButton
                  icon={<SquareTerminal size={12} strokeWidth={2.2} />}
                  label="Open in opencode"
                  onClick={() => void openInOpencode()}
                />
                <RailButton
                  icon={<Code2 size={12} strokeWidth={2.2} />}
                  label="Open in VS Code"
                  onClick={() => void openInVsCode()}
                />
              </>
            )}
          </div>

          {ai.configured && (
            <p className="mt-2.5 text-2xs leading-relaxed text-muted-foreground">
              Runs with <span className="font-semibold text-sub">{ai.provider}</span>
              {ai.model && <> · {ai.model}</>} · uses your {ai.providerShort} plan
            </p>
          )}
        </section>

        {/* With an AI configured this drops to a disclosure: still one click away
            for anyone who lives in opencode, but no longer the first thing. With
            no AI it is the whole workflow and stays open. */}
        {ai.configured ? (
          <details
            className="group mt-4"
            open={handoffOpen}
            onToggle={(e) => setHandoffOpen((e.currentTarget as HTMLDetailsElement).open)}
          >
            <summary className="cursor-pointer list-none text-2xs font-bold tracking-[.1em] text-sub hover:text-foreground">
              <span className="inline-block transition-transform group-open:rotate-90">▸</span>{' '}
              PREFER YOUR OWN EDITOR?
            </summary>
            <p className="mt-2 text-2xs leading-relaxed text-muted-foreground">
              Copy this task to opencode, VS Code, or any AI chat - the same handoff, your
              tool.
            </p>
            <div className="mt-2 flex flex-col gap-2">
              <RailButton
                icon={<ClipboardCopy size={12} strokeWidth={2.4} />}
                label={allDone ? 'Copy review handoff' : 'Copy task handoff'}
                onClick={() => void copyTaskHandoff(change, task)}
              />
              <RailButton
                icon={<SquareTerminal size={12} strokeWidth={2.2} />}
                label="Open in opencode"
                onClick={() => void openInOpencode()}
              />
              <RailButton
                icon={<Code2 size={12} strokeWidth={2.2} />}
                label="Open in VS Code"
                onClick={() => void openInVsCode()}
              />
            </div>
            <h3 className="mb-1.5 mt-3 text-2xs font-bold tracking-[.1em] text-sub">
              WHAT THE AI READS
            </h3>
            <pre className="max-h-56 overflow-auto whitespace-pre-wrap rounded-md border border-border bg-background px-3 py-2.5 font-mono text-2xs leading-relaxed text-sub">
              {handoff}
            </pre>
          </details>
        ) : (
          <section className="mt-4">
            <h3 className="mb-1.5 text-2xs font-bold tracking-[.1em] text-sub">
              WHAT GETS COPIED
            </h3>
            <pre className="max-h-56 overflow-auto whitespace-pre-wrap rounded-md border border-border bg-background px-3 py-2.5 font-mono text-2xs leading-relaxed text-sub">
              {handoff}
            </pre>
            <p className="mt-1.5 text-2xs text-muted-foreground">{repoPath}</p>
          </section>
        )}

        <section className="mt-4">
          <h3 className="mb-1.5 text-2xs font-bold tracking-[.1em] text-sub">
            CLOSE THE LOOP
          </h3>
          <div className="flex flex-col gap-2">
            <RailButton
              icon={<CheckCircle2 size={12} strokeWidth={2.2} />}
              label="Run spec check"
              onClick={runCheck}
              pending={validateChange.isPending}
            />
            {result && <CheckResult outcome={result} />}
            {/* Only shown after the check, so it reads as "what next" rather than
                a warning about something the user has not done. */}
            <RailButton
              icon={<Archive size={12} strokeWidth={2.2} />}
              label="Archive this change…"
              onClick={() => {
                // Say why rather than offering a dead button: the count is the
                // actionable part.
                if (!allDone) {
                  toast.info(
                    change.progress.is_draft
                      ? 'Add tasks and finish them before archiving this change.'
                      : `${remaining} ${remaining === 1 ? 'task is' : 'tasks are'} still open.`
                  )
                  return
                }
                if (change.deltas.length === 0) {
                  toast.info('This change has no requirements yet, so there is nothing to archive into your specs.')
                  return
                }
                setConfirmArchive(true)
              }}
            />
          </div>
        </section>

        {/* One quiet invitation, at the bottom, dismissible. Never a modal, never
            the rail's primary action: the copy workflow above is complete on its
            own, and a nag would imply otherwise. */}
        {ai.state === 'none' && !inviteDismissed && (
          <section className="mt-4 rounded-lg border border-dashed border-border px-3.5 py-3">
            <h3 className="text-2xs font-bold tracking-[.09em] text-sub">
              RUN TASKS RIGHT HERE
            </h3>
            <p className="mt-1.5 text-2xs leading-relaxed text-muted-foreground">
              Connect the AI you already use - Copilot, Anthropic, or a local model - and
              GitWyrm can work on tasks in this window. You watch every step and can stop
              it anytime.
            </p>
            <div className="mt-2.5 flex items-center gap-2">
              <RailButton
                label="Connect an AI"
                onClick={() =>
                  toast.info('Choose an AI in Settings → AI.', {
                    description: 'It lives in the main GitWyrm window, under the gear icon.',
                  })
                }
              />
              <button
                type="button"
                onClick={() => setInviteDismissed(true)}
                className="flex-none px-1 text-2xs text-muted-foreground hover:text-sub"
              >
                Not now
              </button>
            </div>
          </section>
        )}

        {ai.state === 'reconnect' && (
          <section className="mt-4 rounded-lg border border-[var(--gw-amber)]/40 bg-[var(--gw-amber)]/8 px-3.5 py-3">
            <h3 className="text-2xs font-bold tracking-[.09em] text-[var(--gw-amber)]">
              {ai.providerShort.toUpperCase()} NEEDS RECONNECTING
            </h3>
            <p className="mt-1.5 text-2xs leading-relaxed text-[var(--gw-amber)]/85">
              It is signed in but has no models available, so a run could not start.
              Reconnect it in Settings → AI. Copying handoffs works either way.
            </p>
          </section>
        )}
      </div>

      <ConfirmDialog
        open={confirmArchive}
        onOpenChange={setConfirmArchive}
        title={`Archive ${change.id}?`}
        description={
          <>
            Every task is done. Archiving folds this change's{' '}
            {change.deltas.length === 1 ? 'requirement' : 'requirements'} into your specs
            and moves it out of the active list. It stays on disk, under{' '}
            <span className="font-mono">openspec/changes/archive</span>.
          </>
        }
        confirmLabel="Archive it"
        pending={archiveChange.isPending}
        pendingLabel="Archiving…"
        onConfirm={archive}
      />
      <DemoRunLauncher change={change} repoId={repoId} />
    </div>
  )
}

/**
 * The current run, shown in the rail so a gate is visible while the Desk is on
 * another tab. Silent when nothing is running.
 */
function RunBanner({ repoId }: { repoId: string }) {
  const run = useAiRun(repoId)
  if (!run.session || !run.state) return null
  const needsYou = run.state === 'needsYou'
  return (
    <div
      className={cn(
        'mb-3 rounded-lg border px-3 py-2',
        needsYou
          ? 'border-[var(--gw-amber)]/40 bg-[var(--gw-amber)]/8'
          : 'border-border bg-panel2'
      )}
    >
      <div className="flex items-center gap-1.5">
        <span className={cn('flex-none text-2xs', needsYou && 'text-[var(--gw-amber)]')}>
          {stateGlyph(run.state)}
        </span>
        <span
          className={cn(
            'min-w-0 flex-1 truncate text-2xs font-semibold',
            needsYou ? 'text-[var(--gw-amber)]' : 'text-sub'
          )}
        >
          {needsYou ? 'This run needs you' : stateLabel(run.state)}
        </span>
      </div>
      <p className="mt-0.5 line-clamp-2 text-2xs text-muted-foreground">
        Task {run.session.task_number} · {run.session.task_text}
      </p>
    </div>
  )
}

/**
 * Starts a scripted run, for checking the console's states on screen.
 *
 * Development builds only, and labelled as a demo wherever it appears. A
 * scripted run that looked like a real one would be the product lying, so
 * there is deliberately no way to start one without seeing the word "demo".
 */
function DemoRunLauncher({ change, repoId }: { change: SpecChange; repoId: string }) {
  const run = useAiRun(repoId)
  if (import.meta.env.PROD) return null

  const start = async (scenario: DemoScenario) => {
    const task = change.tasks[0]
    const refused = await run.startDemo({
      changeId: change.id,
      taskNumber: 1,
      taskText: task?.text ?? 'Demo task',
      branch: 'main',
      scenario,
    })
    if (refused) toast.info(refused)
  }

  return (
    <div className="mt-4 border-t border-dashed border-border pt-3">
      <p className="mb-1.5 text-2xs font-semibold uppercase tracking-wide text-muted-foreground">
        Demo run (dev only)
      </p>
      <div className="flex flex-wrap gap-1">
        {(
          [
            ['happy', 'Happy'],
            ['gate', 'Gate'],
            ['failure', 'Failure'],
            ['providerExpired', 'Sign-in expired'],
          ] as Array<[DemoScenario, string]>
        ).map(([scenario, label]) => (
          <button
            key={scenario}
            type="button"
            className="rounded border border-border px-1.5 py-0.5 text-2xs text-muted-foreground hover:bg-panel2 hover:text-foreground"
            onClick={() => void start(scenario)}
          >
            {label}
          </button>
        ))}
        {run.session && (
          <button
            type="button"
            className="rounded border border-border px-1.5 py-0.5 text-2xs text-muted-foreground hover:bg-panel2 hover:text-foreground"
            onClick={() => void run.clear()}
          >
            Clear
          </button>
        )}
      </div>
    </div>
  )
}
