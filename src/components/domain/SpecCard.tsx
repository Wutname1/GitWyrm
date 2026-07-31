import { ExternalLink, Sparkles } from 'lucide-react'
import { useSpecAi } from '@/hooks/useSpecAi'
import { stateGlyph, useAiRun } from '@/hooks/useAiRun'
import { cn } from '@/lib/utils'
import { nextStepHint, progressSentence } from '@/lib/specDisplay'
import { ProgressRing, StatusPill } from '@/components/domain/spec-desk/SpecBits'
import { nextTask, useOpenspecStatus, useSelectedChange } from '@/hooks/useOpenspec'
import { copyTaskHandoff } from '@/lib/specHandoff'
import { useActiveRepo } from '@/stores/workspaceStore'
import { openSpecDesk } from '@/lib/specDesk'

/**
 * Spec status for the selected change, pinned above the commit form.
 *
 * The main window stays a git client: this card reports where a change stands
 * and hands work off, but reading proposals, editing tasks, and everything else
 * deep belongs in the Spec Desk. Absent entirely for repos without openspec/,
 * and whenever no change is selected -- deselecting is how you reclaim the room
 * above the commit form.
 */
export function SpecCard() {
  const repo = useActiveRepo()
  const status = useOpenspecStatus(repo?.id ?? null)
  const { change } = useSelectedChange(repo?.id ?? null)
  const ai = useSpecAi()
  const run = useAiRun(repo?.id ?? null)

  if (!repo || !status.data?.present || !change) return null

  const task = nextTask(change)
  const allDone = !task && !change.progress.is_draft

  return (
    <div
      className={cn(
        'flex-none border-b px-3 py-2.5',
        // A gate is the one card state worth changing colour for: the run
        // cannot continue until someone answers it.
        run.state === 'needsYou'
          ? 'border-[var(--gw-amber)]/40 bg-[var(--gw-amber)]/8'
          : 'border-border'
      )}
    >
      {run.session && run.state && (
        <button
          type="button"
          onClick={() => openSpecDesk(repo.id, run.session!.change_id)}
          className={cn(
            'mb-2 flex w-full items-center gap-1.5 rounded px-1.5 py-1 text-left text-2xs',
            run.state === 'needsYou'
              ? 'bg-[var(--gw-amber)]/15 text-[var(--gw-amber)] hover:bg-[var(--gw-amber)]/25'
              : 'bg-panel2 text-muted-foreground hover:text-foreground'
          )}
        >
          <span className="flex-none">{stateGlyph(run.state)}</span>
          {/* Names the task, then what it is doing right now. The task number is
              what makes this legible from another window -- "working" alone does
              not say on what. */}
          <span className="min-w-0 flex-1 truncate">
            {run.state === 'needsYou'
              ? `Task ${run.session.task_number} needs your OK`
              : `The AI is on task ${run.session.task_number}${
                  run.latest ? ` — ${run.latest}` : ''
                }`}
          </span>
          {/* A gate is answered, not watched. Saying "View" for something that is
              blocking would undersell what the click is for. */}
          <span className="flex-none font-semibold">
            {run.state === 'needsYou' ? 'Answer' : 'Watch'}
          </span>
        </button>
      )}
      <div className="flex items-center gap-2">
        <span className="text-2xs font-bold tracking-[.09em] text-sub">SPEC</span>
        <StatusPill status={change.status} />
        <button
          type="button"
          onClick={() => openSpecDesk(repo.id, change.id)}
          className="ml-auto flex items-center gap-1 text-2xs font-semibold text-accent-text hover:text-foreground"
        >
          Open Desk
          <ExternalLink size={10} strokeWidth={2.4} />
        </button>
      </div>

      <p className="mt-1.5 overflow-hidden text-ellipsis whitespace-nowrap font-mono text-xs font-semibold text-foreground">
        {change.id}
      </p>

      <div className="mt-2 flex items-center gap-3">
        <ProgressRing percent={change.progress.percent} />
        <div className="min-w-0">
          <p className="text-xs font-medium text-foreground">{progressSentence(change)}</p>
          {/* The next task is the most useful thing on the card, so it gets the
              room. Wrapped to two lines rather than truncated to one. */}
          <p className="mt-0.5 line-clamp-2 text-2xs leading-snug text-muted-foreground">
            {task ? `Next: ${task.text}` : nextStepHint(change)}
          </p>
        </div>
      </div>

      {/* With an AI configured, running is the primary move and copying drops to
          secondary -- but never disappears. Opening the Desk is the one motion
          that gets you to the run. */}
      {ai.configured && task ? (
        <>
          <button
            type="button"
            onClick={() => void openSpecDesk(repo.id, change.id)}
            className="mt-2.5 flex h-7 w-full items-center justify-center gap-1.5 rounded-md bg-primary text-2xs font-semibold text-primary-foreground transition-[filter] hover:brightness-110"
          >
            <Sparkles size={11} strokeWidth={2.4} />
            Run next task with AI
          </button>
          <button
            type="button"
            onClick={() => copyTaskHandoff(change, task)}
            className="mt-1.5 h-7 w-full rounded-md border border-border bg-panel2 text-2xs font-semibold text-foreground transition-colors hover:border-muted-foreground hover:bg-panel3"
          >
            Copy next-task handoff
          </button>
        </>
      ) : (
        <button
          type="button"
          onClick={() => copyTaskHandoff(change, task)}
          className="mt-2.5 h-7 w-full rounded-md bg-primary text-2xs font-semibold text-primary-foreground transition-[filter] hover:brightness-110"
        >
          {allDone ? 'Copy review handoff' : 'Copy next-task handoff'}
        </button>
      )}
    </div>
  )
}
