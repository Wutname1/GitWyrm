import { useState } from 'react'
import { CircleDot, FileText, ListChecks, Scale } from 'lucide-react'
import { cn } from '@/lib/utils'
import type { SpecChange } from '@/lib/bindings'
import { Markdown } from '@/components/ui/markdown'
import { describeError } from '@/lib/log'
import { useOpenspecArchivedChange, useOpenspecArchivedFile } from '@/hooks/useOpenspec'
import { DeltaBadge } from './SpecBits'

type Tab = 'proposal' | 'deltas' | 'tasks' | 'design'

const TABS: { key: Tab; label: string; icon: typeof FileText }[] = [
  { key: 'proposal', label: 'Proposal', icon: FileText },
  { key: 'deltas', label: 'Spec deltas', icon: Scale },
  { key: 'tasks', label: 'Tasks', icon: ListChecks },
  { key: 'design', label: 'Design', icon: CircleDot },
]

/**
 * One archived change, read-only.
 *
 * Deliberately not the active-change detail with its buttons removed: an
 * archived change has already been folded into the specs library, so ticking a
 * task or editing a file here would rewrite finished work rather than change
 * anything. Everything on this pane reads.
 */
export function ArchivedChangeDetail({
  repoId,
  changeId,
}: {
  repoId: string
  changeId: string
}) {
  const [tab, setTab] = useState<Tab>('proposal')
  const query = useOpenspecArchivedChange(repoId, changeId)
  const change = query.data ?? undefined

  if (query.isLoading) {
    return <p className="px-4 py-3 text-xs text-muted-foreground">Reading this change…</p>
  }

  if (!change) {
    return (
      <p className="px-4 py-3 text-xs text-muted-foreground">
        This change is no longer in the archive folder. Another tool may have moved or
        deleted it.
      </p>
    )
  }

  return (
    <div className="flex min-h-0 flex-col">
      <header className="flex-none border-b border-border px-4 pb-2.5 pt-3">
        <h3 className="text-sm font-semibold text-foreground">{change.title}</h3>
        <p className="mt-0.5 font-mono text-2xs text-muted-foreground">{change.id}</p>
        <p className="mt-1.5 text-2xs text-sub">
          {taskSentence(change)} · {deltaSentence(change)}
        </p>
      </header>

      <div className="flex flex-none gap-3.5 border-b border-border px-4 py-2">
        {TABS.map((t) => (
          <button
            key={t.key}
            type="button"
            onClick={() => setTab(t.key)}
            aria-pressed={tab === t.key}
            className={cn(
              'flex items-center gap-1 border-b pb-0.5 text-2xs transition-colors',
              tab === t.key
                ? 'border-primary text-foreground'
                : 'border-transparent text-muted-foreground hover:text-sub'
            )}
          >
            <t.icon size={11} strokeWidth={2} />
            {t.label}
          </button>
        ))}
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto px-4 py-3">
        {tab === 'proposal' && <ProposalTab change={change} />}
        {tab === 'deltas' && <DeltasTab change={change} />}
        {tab === 'tasks' && <TasksTab change={change} />}
        {tab === 'design' && (
          <DesignTab repoId={repoId} changeId={change.id} hasDesign={change.has_design} />
        )}
      </div>
    </div>
  )
}

/** "12 of 12 tasks done", or the honest thing when there were no tasks. */
function taskSentence(change: SpecChange) {
  const { done, total } = change.progress
  if (total === 0) return 'No tasks were listed'
  return `${done} of ${total} tasks done`
}

function deltaSentence(change: SpecChange) {
  const n = change.deltas.length
  if (n === 0) return 'no spec deltas'
  return `${n} ${n === 1 ? 'requirement set' : 'requirement sets'}`
}

function ProposalTab({ change }: { change: SpecChange }) {
  const hasSections =
    change.proposal.why.trim().length > 0 || change.proposal.what_changes.trim().length > 0

  if (!hasSections) {
    return <Markdown text={change.proposal.raw} empty="proposal.md is empty or missing." />
  }

  return (
    <div className="flex flex-col gap-4">
      <section>
        <h4 className="mb-1.5 text-2xs font-bold tracking-[.1em] text-accent-text">WHY</h4>
        <Markdown text={change.proposal.why} empty="No reason was written." />
      </section>
      <section>
        <h4 className="mb-1.5 text-2xs font-bold tracking-[.1em] text-accent-text">
          WHAT CHANGED
        </h4>
        <Markdown text={change.proposal.what_changes} empty="Not written." />
      </section>
      <section>
        <h4 className="mb-1.5 text-2xs font-bold tracking-[.1em] text-accent-text">IMPACT</h4>
        <Markdown text={change.proposal.impact} empty="Not written." />
      </section>
    </div>
  )
}

function DeltasTab({ change }: { change: SpecChange }) {
  if (change.deltas.length === 0) {
    return (
      <p className="text-xs text-muted-foreground">
        This change recorded no spec deltas.
      </p>
    )
  }
  return (
    <div className="flex flex-col gap-3">
      {change.deltas.map((delta, i) => (
        <section
          key={`${delta.file}-${delta.kind}-${i}`}
          className="overflow-hidden rounded-lg border border-border"
        >
          <header className="flex items-center gap-2.5 bg-panel px-3 py-1.5">
            <DeltaBadge kind={delta.kind} />
            <span className="min-w-0 flex-1 overflow-hidden text-ellipsis whitespace-nowrap font-mono text-2xs text-sub">
              {delta.file}
            </span>
          </header>
          <div className="flex flex-col gap-3 px-3 py-2.5">
            {delta.requirements.map((req) => (
              <div key={req.name}>
                <p className="text-xs font-semibold text-foreground">{req.name}</p>
                {req.text && <Markdown text={req.text} className="mt-1" empty="" />}
                {req.scenarios.map(([name, body]) => (
                  <div key={name} className="mt-2 border-l-2 border-border pl-2.5">
                    <p className="text-2xs font-semibold text-sub">{name}</p>
                    <Markdown text={body} className="mt-0.5" empty="" />
                  </div>
                ))}
              </div>
            ))}
          </div>
        </section>
      ))}
    </div>
  )
}

/**
 * design.md, read from the archive folder.
 *
 * The parsed change carries only `has_design`, so the body is fetched here --
 * and only while this tab is open.
 */
function DesignTab({
  repoId,
  changeId,
  hasDesign,
}: {
  repoId: string
  changeId: string
  hasDesign: boolean
}) {
  const query = useOpenspecArchivedFile(repoId, changeId, 'design.md', hasDesign)

  if (!hasDesign) {
    return <p className="text-xs text-muted-foreground">This change had no design notes.</p>
  }
  if (query.isLoading) {
    return <p className="text-xs text-muted-foreground">Reading design.md…</p>
  }
  if (query.isError) {
    return <p className="text-xs text-removed">{describeError(query.error)}</p>
  }
  return <Markdown text={query.data ?? ''} empty="design.md is empty." />
}

function TasksTab({ change }: { change: SpecChange }) {
  if (change.tasks.length === 0) {
    return <p className="text-xs text-muted-foreground">This change listed no tasks.</p>
  }
  return (
    <ul className="flex flex-col gap-1">
      {change.tasks.map((task) => (
        <li key={task.index} className="flex items-start gap-2 text-xs">
          <span
            aria-hidden
            className={cn(
              'mt-[3px] grid h-3 w-3 flex-none place-items-center rounded-[3px] border text-[8px] font-bold',
              task.done
                ? 'border-primary bg-primary text-background'
                : 'border-muted-foreground/50 text-transparent'
            )}
          >
            ✓
          </span>
          <span className={cn(task.done ? 'text-sub' : 'text-foreground')}>
            <span className="sr-only">{task.done ? 'Done: ' : 'Not done: '}</span>
            {task.text}
          </span>
        </li>
      ))}
    </ul>
  )
}
