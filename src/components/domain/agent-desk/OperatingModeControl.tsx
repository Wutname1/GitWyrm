import { Check, ChevronUp, ShieldCheck } from 'lucide-react'
import { MODE_NOTES, READ_ONLY_REASON, isModeBlocked, type ComposerMode } from '@/lib/agentDeskComposer'
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from '@/components/ui/dropdown-menu'
import { cn } from '@/lib/utils'

/**
 * How much the agent may do, as one chip beside the other composer controls.
 *
 * Was a row: the label "Mode", three always-visible pills, and a note off to
 * the right explaining whichever was selected. That put the three modes and a
 * sentence about them on screen at all times, above a message box, and the
 * first screen showed the same three choices AGAIN as large described cards.
 *
 * A chip instead, for the reason most people do not use this: the default is
 * Auto and stays Auto. Making the exception cost one click, and the rule cost
 * nothing, is the right trade -- and it matches every other control down here
 * (team, AI), so the composer reads as one row of chips rather than a row plus
 * a settings strip.
 *
 * The explanation did not disappear; it moved to where a choice is being made.
 * Each mode carries its own note inside the menu, which is also the only place
 * the notes can be read side by side.
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
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <button
          type="button"
          title={canWrite ? MODE_NOTES[mode] : READ_ONLY_REASON}
          className="flex flex-none items-center gap-1 rounded px-1.5 py-0.5 text-2xs font-semibold text-sub hover:bg-panel3 hover:text-foreground"
        >
          <ShieldCheck size={12} aria-hidden />
          {mode}
          <ChevronUp size={11} aria-hidden />
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent side="top" align="start" className="w-[min(22rem,calc(100vw-3rem))]">
        {(['Ask', 'Plan', 'Auto'] as const).map((m) => {
          const blocked = isModeBlocked(m, canWrite)
          return (
            <DropdownMenuItem
              key={m}
              // `aria-disabled`, not `disabled`, for the same reason
              // `ProviderControl` does it: a truly disabled item leaves the tab
              // order, so the people who most need the reason read aloud are
              // the ones who never reach it.
              aria-disabled={blocked}
              onSelect={(event) => {
                if (blocked) {
                  event.preventDefault()
                  return
                }
                onChange(m)
              }}
              className={cn('items-start gap-2', blocked && 'cursor-not-allowed opacity-70')}
            >
              <span className="min-w-0 flex-1">
                <span className="block text-2xs font-semibold text-foreground">{m}</span>
                <span className="block text-2xs leading-snug text-muted-foreground">
                  {blocked ? READ_ONLY_REASON : MODE_NOTES[m]}
                </span>
              </span>
              {mode === m && !blocked && <Check size={12} className="mt-0.5 flex-none text-accent-text" aria-hidden />}
            </DropdownMenuItem>
          )
        })}
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
