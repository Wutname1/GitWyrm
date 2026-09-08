import { Bot, Check, ChevronDown, FolderGit2, GitFork, Link2, User } from 'lucide-react'
import type { SessionSource } from '@/lib/bindings'
import type { ComposerMode, ComposerTeam } from '@/lib/agentDeskComposer'
import {
  MODE_NOTES,
  READ_ONLY_REASON,
  TEAM_NEEDS_MODE_REASON,
  isModeBlocked,
  isTeamBlocked,
} from '@/lib/agentDeskComposer'
import { sourceKindLabel } from '@/lib/agentSessionGrouping'
import { adapterDisplayName } from '@/lib/agentImportDisplay'
import { cn } from '@/lib/utils'
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from '@/components/ui/dropdown-menu'

export interface ChatProjectChoice {
  path: string
  name: string
}

/**
 * What a chat with nothing in it yet shows.
 *
 * The order is the argument. Any agent client can offer a mode, a team size
 * and a model picker, and leading with those made a new chat look like every
 * other one. What only this app knows is which repository the chat belongs
 * to and what started it (an issue, a pull request, a spec task, a failed
 * check), so those come first and largest. Then the goal, which is the
 * textarea directly below this landing. The AI tool comes after that, and
 * how much authority the agent has is last and quietest: it is a dial on the
 * run, not the point of it.
 *
 * Not a wizard. Every control here is the same state the composer edits, so
 * choosing nothing and simply typing is a complete path -- the defaults are
 * the ones the session was created with.
 */
export function NewChatLanding({
  mode,
  onModeChange,
  team,
  onTeamChange,
  providerLabel,
  onOpenProviderPicker,
  projectPath,
  projectName,
  projects,
  onProjectChange,
  projectChanging = false,
  canWrite = true,
  source,
}: {
  mode: ComposerMode
  onModeChange: (mode: ComposerMode) => void
  team: ComposerTeam
  onTeamChange: (team: ComposerTeam) => void
  /** The chosen tool's name, or the default's, already resolved. */
  providerLabel: string
  onOpenProviderPicker: () => void
  projectPath: string
  projectName: string
  projects: ChatProjectChoice[]
  onProjectChange: (project: ChatProjectChoice) => void
  /** True while a project change is still opening, so the control can say so. */
  projectChanging?: boolean
  /**
   * Whether this chat's purpose allows changing files at all.
   *
   * The composer's own mode pills already refuse Plan and Auto for a chat
   * that only reads -- a Review or Explain chat is read-only in the engine,
   * and mode can never widen what the intent allows. These cards took no
   * such prop, so they sat an inch above those pills offering the same two
   * modes as freely selectable: clicking one lit it up while the engine
   * would refuse it, and the two controls disagreed on screen at once.
   */
  canWrite?: boolean
  /** What started this chat. `null` while the session is still loading. */
  source: SessionSource | null
}) {
  const startedFrom = describeSource(source)
  const teamBlocked = isTeamBlocked(mode)
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-6 px-6 py-8">
      <div className="flex flex-col items-center gap-1.5 text-center">
        <span className="flex size-9 items-center justify-center rounded-full bg-soft">
          <FolderGit2 size={17} className="text-accent-text" aria-hidden />
        </span>
        <h2 className="text-base font-semibold text-foreground">New chat in {projectName}</h2>
        <p className="max-w-sm text-xs leading-relaxed text-muted-foreground">Describe the goal below.</p>
      </div>

      <div className="flex w-full max-w-lg flex-col gap-4">
        <Section label="Which project?">
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <button
                type="button"
                // Opening a project arms a watcher over the whole tree, which
                // can take seconds. Without this the click did nothing
                // visible until it finished.
                disabled={projectChanging}
                className="flex w-full items-center gap-2.5 rounded-md border border-border bg-panel2 px-3 py-2.5 text-left hover:bg-panel3 disabled:cursor-not-allowed disabled:opacity-60"
              >
                <FolderGit2 size={17} className="flex-none text-accent-text" aria-hidden />
                <span className="min-w-0 flex-1">
                  <span className="block truncate text-sm font-semibold text-foreground">{projectName}</span>
                  <span className="block truncate text-2xs text-muted-foreground">{projectPath}</span>
                </span>
                <span className="text-2xs text-muted-foreground">{projectChanging ? 'Opening…' : 'Change'}</span>
                <ChevronDown size={12} className="text-muted-foreground" aria-hidden />
              </button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="start" className="w-[min(30rem,calc(100vw-3rem))]">
              {projects.map((project) => (
                <DropdownMenuItem key={project.path} onSelect={() => onProjectChange(project)}>
                  <FolderGit2 />
                  <span className="min-w-0 flex-1">
                    <span className="block truncate">{project.name}</span>
                    <span className="block truncate text-2xs text-muted-foreground">{project.path}</span>
                  </span>
                  {project.path.toLowerCase() === projectPath.toLowerCase() && <Check size={13} />}
                </DropdownMenuItem>
              ))}
            </DropdownMenuContent>
          </DropdownMenu>
        </Section>

        {/* Only shown when something concrete started the chat. A plain
            manual chat has nothing to say here, and an empty "Started from:
            Chat" row would be noise on the most common path. */}
        {startedFrom && (
          <Section label="What started this?">
            <div className="flex items-center gap-2 rounded-md border border-border px-3 py-2">
              <Link2 size={15} className="flex-none text-muted-foreground" aria-hidden />
              <span className="min-w-0 flex-1">
                <span className="block truncate text-xs font-medium text-foreground">{startedFrom.title}</span>
                {startedFrom.detail && (
                  <span className="block truncate text-2xs text-muted-foreground">{startedFrom.detail}</span>
                )}
              </span>
            </div>
          </Section>
        )}

        <Section label="Which AI?">
          <button
            type="button"
            onClick={onOpenProviderPicker}
            className="flex w-full items-center gap-2 rounded-md border border-border px-3 py-2 text-left hover:bg-panel3"
          >
            <Bot size={15} className="flex-none text-muted-foreground" aria-hidden />
            <span className="flex-1 text-xs font-medium text-foreground">{providerLabel}</span>
            <span className="text-2xs text-muted-foreground">Change</span>
          </button>
        </Section>

        <Section label="How much can it do?">
          <div className="grid grid-cols-3 gap-1.5">
            {(['Ask', 'Plan', 'Auto'] as ComposerMode[]).map((m) => {
              // A chat that only reads cannot be given more authority by
              // picking a mode -- the engine refuses the write tools whatever
              // is selected here. Offering these as freely selectable put
              // this screen in disagreement with the pills directly beneath
              // it, and lit up a choice that would never take effect.
              const blocked = isModeBlocked(m, canWrite)
              return (
                <Choice
                  key={m}
                  selected={mode === m && !blocked}
                  blocked={blocked}
                  onClick={() => {
                    if (!blocked) onModeChange(m)
                  }}
                  title={m}
                  detail={blocked ? READ_ONLY_REASON : MODE_NOTES[m]}
                />
              )
            })}
          </div>
        </Section>

        <Section label="How many agents?">
          <div className="grid grid-cols-2 gap-1.5">
            <Choice
              selected={team === 'solo'}
              onClick={() => onTeamChange('solo')}
              title="One agent"
              detail="One agent does the whole job."
              icon={<User size={14} aria-hidden />}
            />
            {/*
              The same defect the mode cards above were fixed for, eight lines
              down and left alone. In Ask mode the backend adds no instruction
              for handing work to helpers, so a team is a solo run whatever is
              picked here -- and the card above already says "no helpers" in
              its own note. This lit up on click and changed nothing.

              Worse than a choice nobody makes: the team defaults to a team, so
              every read-only chat opened with this selected and the composer
              below reading "A lead agent, up to 3 helpers".
            */}
            <Choice
              selected={team === 'helpers' && !teamBlocked}
              blocked={teamBlocked}
              onClick={() => {
                if (!teamBlocked) onTeamChange('helpers')
              }}
              title="A team"
              detail={teamBlocked ? TEAM_NEEDS_MODE_REASON : 'A lead splits safe work between helpers.'}
              icon={<GitFork size={14} aria-hidden />}
            />
          </div>
        </Section>
      </div>
    </div>
  )
}

/**
 * The source in the words a person would use, with the one detail that
 * identifies it (the issue number, the change id, the commit). Snapshot
 * titles come from the backend at capture time, so they are safe to show as
 * they are; a source with no snapshot title falls back to its kind.
 */
/**
 * "1 file" / "3 files".
 *
 * The two lines below used to read "1 file(s) changed". Every other place
 * that counts files -- `SessionContextPanel` describes these same two
 * sources -- writes it properly, so this was the odd one out, and "(s)" is
 * developer shorthand on the first screen a new person meets.
 */
function fileCount(n: number): string {
  return `${n} file${n === 1 ? '' : 's'}`
}

export function describeSource(source: SessionSource | null): { title: string; detail: string | null } | null {
  if (!source || source.kind === 'manual') return null
  const kind = sourceKindLabel(source.kind)
  switch (source.kind) {
    case 'issue':
    case 'pullRequest':
      return {
        title: source.snapshot.title || `${kind} #${source.number}`,
        detail: `${kind} #${source.number} in ${source.owner}/${source.repo}`,
      }
    case 'openSpecChange':
      return { title: source.snapshot.title || source.changeId, detail: `${kind}: ${source.changeId}` }
    case 'openSpecTask':
      return { title: source.taskText || source.snapshot.title || kind, detail: `${kind} in ${source.changeId}` }
    case 'commit':
      return { title: source.snapshot.title || kind, detail: `${kind} ${source.oid.slice(0, 8)}` }
    case 'diff':
      return { title: source.snapshot.title || kind, detail: `${fileCount(source.paths.length)} changed` }
    case 'workingChanges':
      return {
        title: source.snapshot.title || kind,
        detail: `${fileCount(source.paths.length)} not yet committed`,
      }
    case 'checkFailure':
      return { title: source.snapshot.title || kind, detail: `${kind} from ${source.provider}` }
    case 'imported':
      return { title: source.snapshot.title || kind, detail: `From ${adapterDisplayName(source.adapterId)}` }
  }
}

function Section({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <section className="flex flex-col gap-1.5">
      <h3 className="text-2xs font-semibold uppercase tracking-wide text-muted-foreground">
        {label}
      </h3>
      {children}
    </section>
  )
}

function Choice({
  selected,
  onClick,
  title,
  detail,
  icon,
  blocked = false,
}: {
  selected: boolean
  onClick: () => void
  title: string
  detail: string
  icon?: React.ReactNode
  /**
   * Offered but refused. `aria-disabled` rather than `disabled`, the same
   * choice the composer's pills make: a truly disabled control leaves the
   * tab order, so the reason never reaches the people most relying on it.
   */
  blocked?: boolean
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-disabled={blocked}
      aria-pressed={selected}
      className={cn(
        'flex flex-col gap-0.5 rounded-md border px-2.5 py-2 text-left transition-colors',
        // A tint and a faint border were the whole selected state, with the
        // same text colour either way -- which DESIGN.md's Selected Must Read
        // rule forbids, and this is the first screen a new person meets. The
        // composer's equivalent control already colours its label; this adds
        // that plus a tick, so the choice reads at a glance.
        blocked
          ? 'cursor-not-allowed border-border opacity-55'
          : selected
            ? 'border-primary bg-soft'
            : 'border-border hover:bg-panel3'
      )}
    >
      <span className="flex items-center gap-1.5">
        {icon && <span className={selected ? 'text-accent-text' : 'text-muted-foreground'}>{icon}</span>}
        <span className={cn('text-xs font-semibold', selected ? 'text-accent-text' : 'text-foreground')}>{title}</span>
        {selected && <Check size={12} className="ml-auto flex-none text-accent-text" aria-hidden />}
      </span>
      <span className="text-2xs leading-snug text-muted-foreground">{detail}</span>
    </button>
  )
}
