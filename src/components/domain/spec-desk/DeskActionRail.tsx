import { useEffect, useState } from 'react'
import { Archive, CheckCircle2, ClipboardCopy, SquareTerminal, Code2 } from 'lucide-react'
import { toast } from 'sonner'
import { cn } from '@/lib/utils'
import type { CliOutcome, SpecChange } from '@/lib/bindings'
import { commands } from '@/lib/bindings'
import { unwrap } from '@/lib/queryKeys'
import { composeTaskHandoff, copyTaskHandoff } from '@/lib/specHandoff'
import { nextTask, useOpenspecMutations } from '@/hooks/useOpenspec'
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
}: {
  icon?: React.ReactNode
  label: string
  onClick: () => void
  primary?: boolean
  pending?: boolean
  disabled?: boolean
}) {
  return (
    <button
      type="button"
      onClick={onClick}
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
  const { validateChange, archiveChange } = useOpenspecMutations(repoId)
  const [result, setResult] = useState<CliOutcome | null>(null)
  const [confirmArchive, setConfirmArchive] = useState(false)

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

  return (
    <div className="flex min-h-0 flex-col border-l border-border bg-panel">
      <header className="flex-none border-b border-border px-4 py-3.5">
        <h2 className="text-sm font-semibold text-foreground">Next best action</h2>
        <p className="mt-0.5 text-2xs text-muted-foreground">
          Work from the spec. Keep the tools you already use.
        </p>
      </header>

      <div className="min-h-0 flex-1 overflow-y-auto p-3.5">
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
            The handoff carries this change's proposal, its agreed behavior, and the one
            task - so whatever you paste it into does not have to guess.
          </p>

          <div className="mt-3 flex flex-col gap-2">
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
          </div>
        </section>

        <section className="mt-4">
          <h3 className="mb-1.5 text-2xs font-bold tracking-[.1em] text-sub">
            WHAT GETS COPIED
          </h3>
          <pre className="max-h-56 overflow-auto whitespace-pre-wrap rounded-md border border-border bg-background px-3 py-2.5 font-mono text-2xs leading-relaxed text-sub">
            {handoff}
          </pre>
          <p className="mt-1.5 text-2xs text-muted-foreground">
            {repoPath}
          </p>
        </section>

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
    </div>
  )
}
