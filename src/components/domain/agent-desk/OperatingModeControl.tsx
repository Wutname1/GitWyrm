import { MODE_NOTES, READ_ONLY_REASON, type ComposerMode } from '@/lib/agentDeskComposer'
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
  canWrite = true,
}: {
  mode: ComposerMode
  onChange: (mode: ComposerMode) => void
  /**
   * Whether this chat's purpose allows changing files at all.
   *
   * A chat started from "Explain this issue" or "Review this PR" is read-only
   * in the engine: the policy denies the write and shell tools when the CLI
   * launches, and mode "can never widen what the intent allows". The pills
   * still offered Auto, whose note promises the lead may run commands and
   * change files -- so the control advertised authority that would always be
   * refused, with no explanation of why nothing happened.
   */
  canWrite?: boolean
}) {
  return (
    <div className="mb-1.5 flex flex-wrap items-center gap-1 px-0.5" role="group" aria-label="Agent operating mode">
      <span className="mr-0.5 text-2xs text-muted-foreground">Mode</span>
      {(['Ask', 'Plan', 'Auto'] as const).map((m) => {
        const blocked = !canWrite && m !== 'Ask'
        return (
          <button
            key={m}
            type="button"
            // `aria-disabled`, not `disabled`, for the same reason
            // `ProviderControl` does it: a truly disabled button leaves the tab
            // order, so the people who most need the reason read aloud are the
            // ones who never reach it.
            aria-disabled={blocked}
            title={blocked ? READ_ONLY_REASON : undefined}
            onClick={() => {
              if (!blocked) onChange(m)
            }}
            aria-pressed={mode === m && !blocked}
            className={cn(
              // A tint alone is not a selected state (DESIGN.md), and this is
              // the control that decides whether an agent may change files.
              'rounded border px-1.5 py-0.5 text-2xs font-semibold',
              blocked && 'cursor-not-allowed text-muted-foreground/60',
              !blocked && mode === m
                ? 'border-primary/60 bg-soft text-accent-text'
                : 'border-transparent text-sub',
              !blocked && mode !== m && 'hover:bg-panel3 hover:text-foreground'
            )}
          >
            {m}
          </button>
        )
      })}
      <span className="ml-auto min-w-0 truncate text-2xs text-muted-foreground">
        {canWrite ? MODE_NOTES[mode] : READ_ONLY_REASON}
      </span>
    </div>
  )
}
