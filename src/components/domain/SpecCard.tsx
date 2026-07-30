import { ExternalLink } from 'lucide-react'
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
 * deep belongs in the Spec Desk. Absent entirely for repos without openspec/.
 */
export function SpecCard() {
  const repo = useActiveRepo()
  const status = useOpenspecStatus(repo?.id ?? null)
  const { change } = useSelectedChange(repo?.id ?? null)

  if (!repo || !status.data?.present || !change) return null

  const task = nextTask(change)
  const allDone = !task && !change.progress.is_draft

  return (
    <div className="flex-none border-b border-border px-3 py-2.5">
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

      <button
        type="button"
        onClick={() => copyTaskHandoff(change, task)}
        className="mt-2.5 h-7 w-full rounded-md bg-primary text-2xs font-semibold text-primary-foreground transition-[filter] hover:brightness-110"
      >
        {allDone ? 'Copy review handoff' : 'Copy next-task handoff'}
      </button>
    </div>
  )
}
