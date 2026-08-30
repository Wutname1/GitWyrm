import { Bot, GitFork, Sparkles, User } from 'lucide-react'
import type { ComposerMode, ComposerTeam } from '@/lib/agentDeskComposer'
import { MODE_NOTES } from '@/lib/agentDeskComposer'
import { cn } from '@/lib/utils'

/**
 * What a chat with nothing in it yet shows.
 *
 * The choices that shape a run -- which AI, how much authority it has, how
 * many agents -- were reachable only as small chips in the composer bar. They
 * are the first decisions someone makes and they were the least visible thing
 * on screen, which is backwards. Before a chat has any content there is
 * nothing to compete with them, so they get the middle of the pane and full
 * size; once messages exist the compact controls take over again.
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
}: {
  mode: ComposerMode
  onModeChange: (mode: ComposerMode) => void
  team: ComposerTeam
  onTeamChange: (team: ComposerTeam) => void
  /** The chosen tool's name, or the default's, already resolved. */
  providerLabel: string
  onOpenProviderPicker: () => void
}) {
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-6 px-6 py-8">
      <div className="flex flex-col items-center gap-1.5 text-center">
        <span className="flex size-9 items-center justify-center rounded-full bg-soft">
          <Sparkles size={17} className="text-accent-text" aria-hidden />
        </span>
        <h2 className="text-base font-semibold text-foreground">What do you need done?</h2>
        <p className="max-w-sm text-xs leading-relaxed text-muted-foreground">
          Describe it below, or start from an issue, pull request or spec task in the main window.
        </p>
      </div>

      <div className="flex w-full max-w-lg flex-col gap-4">
        <Section label="How much can it do?">
          <div className="grid grid-cols-3 gap-1.5">
            {(['Ask', 'Plan', 'Auto'] as ComposerMode[]).map((m) => (
              <Choice
                key={m}
                selected={mode === m}
                onClick={() => onModeChange(m)}
                title={m}
                detail={MODE_NOTES[m]}
              />
            ))}
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
            <Choice
              selected={team === 'helpers'}
              onClick={() => onTeamChange('helpers')}
              title="A team"
              detail="A lead splits safe work between helpers."
              icon={<GitFork size={14} aria-hidden />}
            />
          </div>
        </Section>

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
      </div>
    </div>
  )
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
}: {
  selected: boolean
  onClick: () => void
  title: string
  detail: string
  icon?: React.ReactNode
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-pressed={selected}
      className={cn(
        'flex flex-col gap-0.5 rounded-md border px-2.5 py-2 text-left transition-colors',
        selected
          ? 'border-primary/60 bg-soft'
          : 'border-border hover:bg-panel3'
      )}
    >
      <span className="flex items-center gap-1.5">
        {icon && <span className="text-muted-foreground">{icon}</span>}
        <span className="text-xs font-semibold text-foreground">{title}</span>
      </span>
      <span className="text-[10.5px] leading-snug text-muted-foreground">{detail}</span>
    </button>
  )
}
