import { MODE_NOTES, type ComposerMode } from '@/lib/agentDeskComposer'
import { cn } from '@/lib/utils'

/**
 * Ask / Plan / Auto operating-mode pills, plus a live plain-language note for
 * whichever mode is selected (tasks.md 6.1).
 *
 * Structure mirrors the real mockup's `.ag-mode-row` (`role="group"`, a
 * `.ag-mode-label`, three `.ag-mode` buttons, a trailing `.ag-mode-note`) --
 * see `docs/agent-desk/agent-desk-mockup.html`. Colors come from the app's
 * own tokens rather than the mockup's `--ag-*` palette.
 */
export function OperatingModeControl({
  mode,
  onChange,
}: {
  mode: ComposerMode
  onChange: (mode: ComposerMode) => void
}) {
  return (
    <div className="mb-1.5 flex flex-wrap items-center gap-1 px-0.5" role="group" aria-label="Agent operating mode">
      <span className="mr-0.5 text-2xs text-muted-foreground">Mode</span>
      {(['Ask', 'Plan', 'Auto'] as const).map((m) => (
        <button
          key={m}
          type="button"
          onClick={() => onChange(m)}
          aria-pressed={mode === m}
          className={cn(
            // A tint alone is not a selected state (DESIGN.md), and this is
            // the control that decides whether an agent may change files.
            'rounded border px-1.5 py-0.5 text-2xs font-semibold',
            mode === m
              ? 'border-primary/60 bg-soft text-accent-text'
              : 'border-transparent text-sub hover:bg-panel3 hover:text-foreground'
          )}
        >
          {m}
        </button>
      ))}
      <span className="ml-auto min-w-0 truncate text-2xs text-muted-foreground">{MODE_NOTES[mode]}</span>
    </div>
  )
}
