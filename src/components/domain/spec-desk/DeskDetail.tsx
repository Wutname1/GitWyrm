import { useEffect, useState } from 'react'
import { Bot, Check, CircleDot, FileText, GitCommitHorizontal, ListChecks, Scale } from 'lucide-react'
import { toast } from 'sonner'
import { cn } from '@/lib/utils'
import type { SpecChange, SpecTask } from '@/lib/bindings'
import { commands } from '@/lib/bindings'
import { Markdown } from '@/components/ui/markdown'
import { TooltipHint } from '@/components/ui/tooltip'
import { formatCommitTime, formatRelativeTime } from '@/lib/gitDisplay'
import { nextStepHint, progressSentence } from '@/lib/specDisplay'
import { copyTaskHandoff } from '@/lib/specHandoff'
import { nextTask, useOpenspecHistory, useOpenspecMutations } from '@/hooks/useOpenspec'
import { stateGlyph, useAiRun } from '@/hooks/useAiRun'
import { AiRunTab } from '@/components/domain/ai-run/AiRunTab'
import { AskTab } from '@/components/domain/ai-run/AskTab'
import { useAskStore } from '@/stores/askStore'
import { useStartRun } from '@/hooks/useStartRun'
import { useSpecAi } from '@/hooks/useSpecAi'
import { ConfirmDialog } from '@/components/modals/ConfirmDialog'
import { unwrap } from '@/lib/queryKeys'
import { describeError } from '@/lib/log'
import { DeltaBadge, ProgressRing, StatusPill } from './SpecBits'
import { EditFileButton, SpecFileEditor } from './SpecFileEditor'
import { DraftedEditReview } from './DraftedEditReview'
import { useSpecDraftStore } from '@/stores/specDraftStore'

type Tab = 'overview' | 'proposal' | 'deltas' | 'history' | 'ai'

/** A card with a small uppercase header, used across the overview. */
function Card({
  title,
  aside,
  children,
}: {
  title: string
  aside?: React.ReactNode
  children: React.ReactNode
}) {
  return (
    <section className="rounded-lg border border-border bg-background/40">
      <header className="flex items-center justify-between gap-2 border-b border-border px-3.5 py-2">
        <h3 className="text-2xs font-bold tracking-[.09em] text-sub">{title}</h3>
        {aside}
      </header>
      <div className="px-3.5 py-3">{children}</div>
    </section>
  )
}

/** The four artifacts a change is made of, and whether each exists. */
function ChangePackage({ change }: { change: SpecChange }) {
  const items = [
    {
      icon: FileText,
      label: 'Proposal',
      present: change.proposal.why.trim().length > 0 || change.proposal.raw.trim().length > 0,
      detail: 'Why this matters and what people will notice.',
    },
    {
      icon: Scale,
      label: 'Spec deltas',
      present: change.deltas.length > 0,
      detail:
        change.deltas.length > 0
          ? `${change.deltas.length} ${change.deltas.length === 1 ? 'requirement set' : 'requirement sets'} this change edits.`
          : 'Written during the proposal step.',
    },
    {
      icon: CircleDot,
      label: 'Design',
      present: change.has_design,
      detail: change.has_design
        ? 'Decisions, edge cases, and paths not taken.'
        : 'Optional - add design.md for tricky choices.',
    },
    {
      icon: ListChecks,
      label: 'Tasks',
      present: change.tasks.length > 0,
      detail:
        change.tasks.length > 0
          ? `${change.tasks.length} steps that make progress visible.`
          : 'Add checkboxes to tasks.md.',
    },
  ]

  return (
    <div className="grid grid-cols-2 gap-2 lg:grid-cols-4">
      {items.map((item) => (
        <div
          key={item.label}
          className={cn(
            'rounded-lg border px-3 py-2.5',
            item.present ? 'border-border bg-background/40' : 'border-dashed border-border'
          )}
        >
          <div className="flex items-center gap-1.5">
            <item.icon
              size={12}
              strokeWidth={2}
              className={item.present ? 'text-accent-text' : 'text-muted-foreground'}
            />
            <span className="text-2xs font-semibold text-foreground">{item.label}</span>
            {item.present ? (
              <Check size={11} strokeWidth={3} className="ml-auto text-accent-text" />
            ) : (
              <span className="ml-auto text-2xs text-muted-foreground">–</span>
            )}
          </div>
          <p className="mt-1.5 text-2xs leading-snug text-muted-foreground">{item.detail}</p>
        </div>
      ))}
    </div>
  )
}

/** One row of the task list. */
function TaskRow({
  task,
  isNext,
  onToggle,
  onCopy,
  pending,
}: {
  task: SpecTask
  isNext: boolean
  onToggle: () => void
  onCopy: () => void
  pending: boolean
}) {
  return (
    <div
      className={cn(
        'group/task flex items-start gap-2.5 px-3.5 py-2 transition-colors',
        isNext && 'bg-[var(--gw-blue)]/5',
        !task.done && 'hover:bg-panel'
      )}
    >
      <button
        type="button"
        onClick={onToggle}
        disabled={pending}
        aria-pressed={task.done}
        aria-label={task.done ? 'Mark this task not done' : 'Mark this task done'}
        className={cn(
          'mt-px grid size-[15px] flex-none place-items-center rounded border transition-colors disabled:opacity-50',
          task.done
            ? 'border-primary bg-primary text-primary-foreground'
            : 'border-muted-foreground bg-panel hover:border-accent-text'
        )}
      >
        {task.done && <Check size={10} strokeWidth={3.5} />}
      </button>

      <span
        className={cn(
          'min-w-0 flex-1 text-xs leading-snug',
          task.done ? 'text-muted-foreground line-through decoration-panel3' : 'text-foreground'
        )}
      >
        {task.text}
      </span>

      {!task.done && (
        <button
          type="button"
          onClick={onCopy}
          className="flex-none whitespace-nowrap font-mono text-2xs text-accent-text opacity-0 transition-opacity hover:underline focus:opacity-100 group-hover/task:opacity-100"
        >
          Copy handoff →
        </button>
      )}

      <span className="flex-none pt-px text-2xs text-muted-foreground">
        {task.done ? 'Done' : isNext ? <span className="font-semibold text-[var(--gw-blue)]">Ready now</span> : 'Later'}
      </span>
    </div>
  )
}

/** tasks.md as a grouped, clickable checklist. */
function TaskList({ change, repoId }: { change: SpecChange; repoId: string }) {
  const { toggleTask } = useOpenspecMutations(repoId)
  const next = nextTask(change)

  if (change.tasks.length === 0) {
    return (
      <p className="px-3.5 py-3 text-xs text-muted-foreground">
        No tasks yet - add them in tasks.md (plain markdown).
      </p>
    )
  }

  // Group headings come from the file; render each once, in file order.
  const groups: Array<{ name: string; tasks: SpecTask[] }> = []
  for (const task of change.tasks) {
    const last = groups[groups.length - 1]
    if (last && last.name === task.group) last.tasks.push(task)
    else groups.push({ name: task.group, tasks: [task] })
  }

  return (
    <div className="overflow-hidden rounded-lg border border-border bg-background/40">
      {groups.map((group, index) => (
        <div key={`${group.name}-${index}`}>
          {group.name && (
            <p
              className={cn(
                'px-3.5 pb-1 pt-2.5 text-2xs font-bold tracking-[.09em] text-muted-foreground',
                index > 0 && 'border-t border-border'
              )}
            >
              {group.name.toUpperCase()}
            </p>
          )}
          {group.tasks.map((task) => (
            <TaskRow
              key={task.index}
              task={task}
              isNext={task === next}
              pending={toggleTask.isPending}
              onToggle={() =>
                toggleTask.mutate(
                  { changeId: change.id, line: task.line, done: !task.done },
                  {
                    onSuccess: (outcome) => {
                      // The file can move under us when an agent or editor is
                      // also writing it; say so rather than looking ignored.
                      if (outcome === 'lineMoved') {
                        toast.info('tasks.md changed just now - reloaded it, try again.')
                      }
                    },
                    onError: (e) => toast.error(String(e)),
                  }
                )
              }
              onCopy={() => copyTaskHandoff(change, task)}
            />
          ))}
        </div>
      ))}
    </div>
  )
}

/** Commits that touched this change's folder. */
function HistoryTab({ change, repoId }: { change: SpecChange; repoId: string }) {
  const history = useOpenspecHistory(repoId, change.id)
  const entries = history.data ?? []

  if (history.isLoading) {
    return <p className="text-xs text-muted-foreground">Reading history…</p>
  }
  if (entries.length === 0) {
    return (
      <p className="text-xs text-muted-foreground">
        Nothing committed yet. Commits that touch this change's files show up here.
      </p>
    )
  }
  return (
    <div className="max-w-2xl">
      {entries.map((entry) => (
        <div
          key={entry.short_sha}
          className="grid grid-cols-[18px_1fr_auto] items-start gap-2.5 border-b border-border/60 py-2.5 last:border-b-0"
        >
          <span className="mt-px grid size-[18px] place-items-center rounded-full bg-soft">
            {entry.ai_assisted ? (
              <Bot size={10} strokeWidth={2.2} className="text-accent-text" />
            ) : (
              <GitCommitHorizontal size={11} strokeWidth={2.2} className="text-accent-text" />
            )}
          </span>
          <span className="min-w-0 text-xs leading-snug text-foreground">
            {entry.summary}
            {entry.ai_assisted && (
              <span className="ml-1.5 text-2xs text-muted-foreground">
                · with AI · reviewed by {entry.author}
              </span>
            )}
          </span>
          <TooltipHint label={formatCommitTime(entry.time)}>
            <span className="flex-none whitespace-nowrap pt-px font-mono text-2xs text-muted-foreground">
              {formatRelativeTime(entry.time)} · {entry.author}
            </span>
          </TooltipHint>
        </div>
      ))}
    </div>
  )
}

const TABS: Array<{ key: Tab; label: string }> = [
  { key: 'overview', label: 'Overview' },
  { key: 'proposal', label: 'Proposal' },
  { key: 'deltas', label: 'Spec deltas' },
  { key: 'history', label: 'History' },
]

/**
 * The Desk's center column: a change's header, its tabs, and its task list.
 *
 * Every tab renders the same change the header names -- the header is not allowed
 * to describe one change while the content below shows another.
 */
export function DeskDetail({ change, repoId }: { change: SpecChange; repoId: string }) {
  const [tab, setTab] = useState<Tab>('overview')
  const run = useAiRun(repoId)
  const askSession = useAskStore((s) => s.byRepo[repoId])
  const { startRun, canStart: canStartRun } = useStartRun(repoId, change)
  const ai = useSpecAi()
  const [confirmUndo, setConfirmUndo] = useState(false)
  const [undoing, setUndoing] = useState(false)
  /**
   * The file currently open in the editor, relative to the change folder, or
   * null when every tab is rendering read-only.
   *
   * One at a time: two editors on screen would make "Save" ambiguous, and the
   * drafts of the others are held in the store regardless, so nothing is lost by
   * closing one to open another.
   */
  const [editing, setEditing] = useState<string | null>(null)
  /**
   * An AI-drafted edit waiting to be looked at, with the file's contents as they
   * were when it was drafted so the diff compares against what the user saw.
   */
  const [drafted, setDrafted] = useState<{
    file: string
    body: string
    summary: string
    current: string
  } | null>(null)
  const [drafting, setDrafting] = useState(false)
  const replaceDraft = useSpecDraftStore((s) => s.replace)
  const changeHasDirty = useSpecDraftStore((s) =>
    Object.entries(s.drafts).some(
      ([key, draft]) =>
        key.startsWith(`${repoId} ${change.id} `) && draft != null && draft.text !== draft.original
    )
  )

  // Selecting another change closes the editor. The draft stays in the store, so
  // coming back reopens it with the unsaved text intact -- but leaving an editor
  // open across changes would show one change's file under another's header.
  useEffect(() => {
    setEditing(null)
  }, [change.id])

  /**
   * Throw away the AI's edits and put its task back to not-done.
   *
   * Order matters: discard first, un-tick second. A tick left standing over
   * discarded work would claim a task was done when its changes are gone, which
   * is worse than the reverse.
   */
  const undoRun = async () => {
    const session = run.session
    if (!session || undoing) return
    setUndoing(true)
    try {
      unwrap(await commands.discardAll(repoId))
      const line = change.tasks.find((t) => t.text === session.task_text)?.line
      if (line != null) {
        await commands.openspecToggleTask(repoId, session.change_id, line, false)
      }
      await run.clear()
      setConfirmUndo(false)
      toast.success("Undone. The AI's edits are gone and the task is open again.")
    } catch (e) {
      toast.error('Could not undo that.', { description: describeError(e) })
    } finally {
      setUndoing(false)
    }
  }
  /**
   * Ask the AI for a rewrite of one file. Writes nothing.
   *
   * The file is read first so the review can diff against what is actually on
   * disk right now, rather than against whatever the AI was shown -- those can
   * differ if the file changed while the question was being typed.
   */
  const draftEdit = async (file: string, instruction: string) => {
    if (drafting || !ai.provider || !ai.model) return
    setDrafting(true)
    try {
      const current = unwrap(await commands.openspecReadFile(repoId, change.id, file))
      const draft = unwrap(
        await commands.openspecDraftEdit(
          repoId,
          change.id,
          file,
          instruction,
          ai.provider,
          ai.model
        )
      )
      setDrafted({ file: draft.file, body: draft.body, summary: draft.summary, current })
    } catch (e) {
      toast.error('That edit could not be drafted.', { description: describeError(e) })
    } finally {
      setDrafting(false)
    }
  }

  /**
   * Take a drafted edit into the editor as unsaved text.
   *
   * Deliberately not a write: the AI's wording lands where the user can change
   * it, and reaches disk by the same Save they use for a hand edit. One write
   * path, one path refusal.
   */
  const acceptDraft = () => {
    if (!drafted) return
    replaceDraft(repoId, change.id, drafted.file, drafted.body)
    setEditing(drafted.file)
    setTab(drafted.file === 'proposal.md' ? 'proposal' : drafted.file.startsWith('specs/') ? 'deltas' : 'overview')
    setDrafted(null)
    toast.success(`Opened in the editor. Nothing is saved until you press Save.`)
  }

  // An ask session belongs to the change it was started on. Showing one change's
  // answers under another's header would make every citation a lie.
  const asking = askSession?.changeId === change.id
  // The AI tab only exists once there is something to show. An empty tab that is
  // always present invites clicking on nothing.
  //
  // A run wins the tab when both exist: it is the mode with state to watch and
  // possibly a gate waiting, and it is the one that can change files.
  const hasRun = run.session != null
  const askMode = asking && !hasRun
  const showAiTab = hasRun || asking
  const tabs = showAiTab
    ? [...TABS, { key: 'ai' as Tab, label: askMode ? '✦ Ask AI' : '✦ AI' }]
    : TABS

  // Leaving the change the ask belongs to sends the tab back somewhere real,
  // rather than leaving an empty ✦ selected.
  useEffect(() => {
    if (tab === 'ai' && !showAiTab) setTab('overview')
  }, [tab, showAiTab])
  const history = useOpenspecHistory(repoId, change.id, tab === 'overview' || tab === 'history')
  const recent = (history.data ?? []).slice(0, 3)

  return (
    <div className="flex min-h-0 flex-col">
      <div className="flex-none px-6 pt-5">
        <p className="font-mono text-2xs text-muted-foreground">
          openspec / changes / {change.id}
          {/* Unsaved work is easy to forget once the editor is closed, so the
              change itself carries the marker, not only the file. */}
          {changeHasDirty && (
            <span className="ml-2 text-[var(--gw-amber)]">. unsaved changes</span>
          )}
        </p>
        <div className="mt-1.5 flex items-start justify-between gap-4">
          <h1 className="text-lg font-semibold leading-tight text-foreground">{change.title}</h1>
          <StatusPill status={change.status} className="mt-1" />
        </div>
        {change.proposal.why.trim() && (
          <p className="mt-1.5 max-w-2xl text-xs leading-relaxed text-sub">
            {/* The first line of Why is the change's one-sentence goal. */}
            {change.proposal.why.trim().split('\n')[0]}
          </p>
        )}
        {change.notes.length > 0 && (
          <p className="mt-2 rounded border border-[var(--gw-amber)]/40 bg-[var(--gw-amber)]/8 px-2.5 py-1.5 text-2xs text-[var(--gw-amber)]">
            {change.notes.join(' · ')}
          </p>
        )}
      </div>

      <div
        role="tablist"
        aria-label="Change details"
        className="mt-4 flex flex-none gap-5 border-b border-border px-6"
      >
        {tabs.map((t) => (
          <button
            key={t.key}
            role="tab"
            aria-selected={tab === t.key}
            onClick={() => setTab(t.key)}
            className={cn(
              '-mb-px border-b-2 pb-2 text-xs font-semibold transition-colors',
              tab === t.key
                ? 'border-primary text-foreground'
                : 'border-transparent text-sub hover:text-foreground'
            )}
          >
            {t.label}
            {t.key === 'deltas' && (
              <span className="ml-1.5 rounded-full bg-panel3 px-1.5 font-mono text-2xs font-normal text-sub">
                {change.deltas.length}
              </span>
            )}
            {t.key === 'ai' && run.state && (
              <span
                className={cn(
                  'ml-1.5 font-normal',
                  run.state === 'needsYou' ? 'text-[var(--gw-amber)]' : 'text-sub'
                )}
                // The badge is how a gate reaches someone reading another tab.
                aria-label={run.state === 'needsYou' ? 'This run needs you' : undefined}
              >
                {stateGlyph(run.state)}
              </span>
            )}
          </button>
        ))}
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto px-6 py-5">
        {tab === 'overview' && (
          <div className="flex flex-col gap-5">
            <div className="grid gap-3 lg:grid-cols-2">
              <Card title="Progress · from tasks.md" aside={
                <span className="font-mono text-2xs text-muted-foreground">
                  {progressSentence(change)}
                </span>
              }>
                <div className="flex items-center gap-3.5">
                  <ProgressRing percent={change.progress.percent} size={50} />
                  <div className="min-w-0">
                    <p className="text-xs font-semibold text-foreground">
                      {progressSentence(change)}
                    </p>
                    <p className="mt-0.5 text-2xs leading-snug text-muted-foreground">
                      {nextStepHint(change)}
                    </p>
                  </div>
                </div>
              </Card>

              <Card title="Latest activity">
                {recent.length === 0 ? (
                  <p className="text-2xs text-muted-foreground">
                    Nothing committed for this change yet.
                  </p>
                ) : (
                  <div className="flex flex-col gap-2">
                    {recent.map((entry) => (
                      <div
                        key={entry.short_sha}
                        className="grid grid-cols-[16px_1fr_auto] items-center gap-2 text-2xs"
                      >
                        <span className="grid size-4 place-items-center rounded-full bg-soft text-accent-text">
                          {entry.ai_assisted ? <Bot size={9} /> : <Check size={9} strokeWidth={3} />}
                        </span>
                        <span className="min-w-0 overflow-hidden text-ellipsis whitespace-nowrap text-foreground">
                          {entry.summary}
                        </span>
                        <span className="flex-none font-mono text-muted-foreground">
                          {formatRelativeTime(entry.time)}
                        </span>
                      </div>
                    ))}
                  </div>
                )}
              </Card>
            </div>

            <div>
              <h2 className="mb-2 text-2xs font-bold tracking-[.1em] text-sub">CHANGE PACKAGE</h2>
              <ChangePackage change={change} />
            </div>

            <div>
              <div className="mb-2 flex items-center justify-between gap-3">
                <h2 className="text-2xs font-bold tracking-[.1em] text-sub">TASKS</h2>
                {editing !== 'tasks.md' && (
                  <EditFileButton onClick={() => setEditing('tasks.md')} label="Edit tasks.md" />
                )}
              </div>
              {editing === 'tasks.md' ? (
                <SpecFileEditor
                  repoId={repoId}
                  changeId={change.id}
                  file="tasks.md"
                  onClose={() => setEditing(null)}
                />
              ) : (
                <TaskList change={change} repoId={repoId} />
              )}
            </div>

            <div>
              <div className="mb-2 flex items-center justify-between gap-3">
                <h2 className="text-2xs font-bold tracking-[.1em] text-sub">DESIGN</h2>
                {editing !== 'design.md' && (
                  <EditFileButton onClick={() => setEditing('design.md')} label="Edit design.md" />
                )}
              </div>
              {editing === 'design.md' ? (
                <SpecFileEditor
                  repoId={repoId}
                  changeId={change.id}
                  file="design.md"
                  onClose={() => setEditing(null)}
                />
              ) : (
                <p className="text-2xs text-muted-foreground">
                  {change.has_design
                    ? 'design.md holds the technical notes for this change.'
                    : 'No design.md yet. Editing creates one.'}
                </p>
              )}
            </div>
          </div>
        )}

        {tab === 'proposal' && editing === 'proposal.md' && (
          <div className="max-w-2xl">
            <SpecFileEditor
              repoId={repoId}
              changeId={change.id}
              file="proposal.md"
              onClose={() => setEditing(null)}
            />
          </div>
        )}

        {tab === 'proposal' && editing !== 'proposal.md' && (
          <div className="max-w-2xl">
            <div className="mb-3 flex justify-end">
              <EditFileButton onClick={() => setEditing('proposal.md')} label="Edit proposal.md" />
            </div>
            {/* Sections when the file has the expected headings; the raw file when
                it does not, so a half-written proposal still shows its content. */}
            {change.proposal.why.trim() || change.proposal.what_changes.trim() ? (
              <div className="flex flex-col gap-4">
                <section>
                  <h2 className="mb-1.5 text-2xs font-bold tracking-[.1em] text-accent-text">WHY</h2>
                  <Markdown text={change.proposal.why} empty="No reason written yet." />
                </section>
                <section>
                  <h2 className="mb-1.5 text-2xs font-bold tracking-[.1em] text-accent-text">
                    WHAT CHANGES
                  </h2>
                  <Markdown text={change.proposal.what_changes} empty="Not written yet." />
                </section>
                <section>
                  <h2 className="mb-1.5 text-2xs font-bold tracking-[.1em] text-accent-text">
                    IMPACT
                  </h2>
                  <Markdown text={change.proposal.impact} empty="Not written yet." />
                </section>
              </div>
            ) : (
              <>
                <p className="mb-3 rounded border border-border px-2.5 py-1.5 text-2xs text-muted-foreground">
                  This proposal does not use the usual Why / What Changes / Impact headings, so
                  it is shown as written.
                </p>
                <Markdown
                  text={change.proposal.raw}
                  empty="proposal.md is empty or missing."
                />
              </>
            )}
          </div>
        )}

        {tab === 'deltas' && editing?.startsWith('specs/') && (
          <div className="max-w-2xl">
            <SpecFileEditor
              repoId={repoId}
              changeId={change.id}
              file={editing}
              onClose={() => setEditing(null)}
            />
          </div>
        )}

        {tab === 'deltas' && !editing?.startsWith('specs/') && (
          <div className="max-w-2xl">
            {change.deltas.length === 0 ? (
              <p className="text-xs text-muted-foreground">
                No spec deltas yet. They are written during the proposal step, and a change needs
                at least one before it can be archived.
              </p>
            ) : (
              <div className="flex flex-col gap-3">
                {change.deltas.map((delta, i) => (
                  <section
                    key={`${delta.file}-${delta.kind}-${i}`}
                    className="overflow-hidden rounded-lg border border-border"
                  >
                    <header className="flex items-center gap-2.5 bg-panel px-3.5 py-2">
                      <DeltaBadge kind={delta.kind} />
                      <span className="min-w-0 flex-1 overflow-hidden text-ellipsis whitespace-nowrap font-mono text-2xs text-sub">
                        {delta.file}
                      </span>
                      <EditFileButton onClick={() => setEditing(delta.file)} />
                    </header>
                    <div className="flex flex-col gap-3 px-3.5 py-3">
                      {delta.requirements.map((req) => (
                        <div key={req.name}>
                          <p className="text-xs font-semibold text-foreground">{req.name}</p>
                          {req.text && (
                            <Markdown text={req.text} className="mt-1" empty="" />
                          )}
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
            )}
          </div>
        )}

        {tab === 'history' && <HistoryTab change={change} repoId={repoId} />}
        {tab === 'ai' && drafting && (
          <p className="mb-3 rounded border border-border px-2.5 py-1.5 text-2xs text-muted-foreground">
            Drafting that edit. Nothing is being changed yet.
          </p>
        )}
        {tab === 'ai' && drafted && (
          <div className="mb-4">
            <DraftedEditReview
              file={drafted.file}
              summary={drafted.summary}
              current={drafted.current}
              proposed={drafted.body}
              onAccept={acceptDraft}
              onReject={() => setDrafted(null)}
            />
          </div>
        )}
        {tab === 'ai' && askMode && (
          <AskTab
            repoId={repoId}
            changeId={change.id}
            // A citation chip opens the tab holding the document it names, which
            // is what makes a cited answer checkable rather than just decorated.
            onOpenTab={(next) => {
              if (next === 'proposal' || next === 'deltas' || next === 'history') {
                setTab(next as Tab)
              } else {
                // tasks and design live under Overview.
                setTab('overview')
              }
            }}
            // Promotion from read-only to write, as one explicit click. Absent
            // when every task is done, so the button is never offered with
            // nothing to run.
            onStartRun={
              canStartRun
                ? () => {
                    void startRun()
                    setTab('ai')
                  }
                : undefined
            }
            // Drafting an edit is offered only when a provider is resolved --
            // otherwise the button would be there with nothing behind it.
            onDraftEdit={
              ai.provider && ai.model ? (file, instruction) => void draftEdit(file, instruction) : undefined
            }
            deltaFiles={change.deltas.map((d) => d.file)}
          />
        )}
        {tab === 'ai' && !askMode && (
          <AiRunTab
            run={run}
            // Names the AI on the commit's Assisted-by trailer. Absent when no
            // provider is resolved, which makes the run fall back to the plain
            // ending rather than drafting a commit that misattributes itself.
            provider={ai.provider || undefined}
            endingActions={{
              // Keeping is the default state already: the edits are sitting in
              // the changes list, so this just closes the run out.
              onKeep: () => {
                void run.clear()
                toast.success('Kept. The changes are in your changes list.')
              },
              // The run is done once its work is committed. Clearing frees the
              // repository to start the next task, which is an explicit click --
              // runs never chain themselves.
              onCommitted: (sha) => {
                void run.clear()
                toast.success(`Committed ${sha.slice(0, 7)}.`, {
                  description: 'The task is ticked and the commit is on your branch.',
                })
              },
              // Throws the AI's edits away and puts the task back to not-done.
              // Confirmed first: this is the one ending action that destroys
              // work, and a misclick here costs the whole run.
              onUndo: () => setConfirmUndo(true),
              // Restarts the run's own task, not whatever is selected now --
              // the session carries its change and task for exactly this.
              onRestart: () => {
                void (async () => {
                  const session = run.session
                  if (!session) return
                  await run.clear()
                  const res = await commands.aiRunStart(
                    repoId,
                    session.change_id,
                    // The session does not carry the checkbox index, so the
                    // next unticked task in this change is the target. That is
                    // the same task unless it was ticked during the run.
                    nextTask(change)?.index ?? 0,
                    session.task_number,
                    session.task_text,
                    session.branch
                  )
                  if (res.status !== 'ok') {
                    toast.error('That could not be restarted.', { description: res.error })
                  }
                })()
              },
              // These two can act for real: reconnecting is a settings trip,
              // and the handoff is the escape hatch that exists precisely for
              // when the AI cannot run.
              // Settings live in the main window, so this brings that window
              // forward and says where to go rather than pretending the Desk
              // can open a panel it does not have.
              onReconnect: () => {
                void (async () => {
                  try {
                    const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow')
                    const main = await WebviewWindow.getByLabel('main')
                    await main?.unminimize()
                    await main?.show()
                    await main?.setFocus()
                  } catch {
                    // Falling through to the toast is fine: the instruction is
                    // the useful part, not the focus change.
                  }
                  toast.info('Open Settings > AI in the main window to sign in again.')
                })()
              },
              onCopyHandoff: () => {
                const task = nextTask(change)
                if (task) void copyTaskHandoff(change, task)
              },
              // Adding a note to a retry needs somewhere to type it, which is
              // its own piece of UI. Restart plus the composer already covers
              // the same ground, so this is left out rather than stubbed.
              onRetryWithNote: undefined,
            }}
          />
        )}
      </div>

      <ConfirmDialog
        open={confirmUndo}
        onOpenChange={setConfirmUndo}
        title="Undo the AI's edits?"
        description={
          <>
            Every file the AI changed goes back to how it was, and its task is marked
            not done again. Anything you changed yourself in those files goes too, so
            this is worth a look at your changes list first.
          </>
        }
        confirmLabel="Undo the edits"
        destructive
        pending={undoing}
        pendingLabel="Undoing…"
        onConfirm={() => void undoRun()}
      />
    </div>
  )
}
