import { forwardRef, type ComponentPropsWithoutRef } from 'react'
import { Check, Power, Settings2, Sparkles } from 'lucide-react'
import { toast } from 'sonner'
import { cn } from '@/lib/utils'
import { useAiCatalog } from '@/hooks/useAi'
import { useSpecAi } from '@/hooks/useSpecAi'
import { isActive, useAiRun } from '@/hooks/useAiRun'
import { openAiSettings } from '@/lib/openAiSettings'
import { ProviderGlyph } from '@/lib/brandLogos'
import { useWorkspaceStore } from '@/stores/workspaceStore'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { TooltipHint } from '@/components/ui/tooltip'
import { aiChipNoun, aiChipOffDescription, aiChipScopeNote, type AiChipScope } from '@/lib/aiChipScope'

/**
 * Which AI writes for you, and the switch for it, in the titlebar.
 *
 * It is deliberately always present -- a missing chip would leave "is an AI
 * involved here?" unanswered.
 *
 * It is also the control. With one provider set up a click turns the AI off and
 * on; with several it opens a picker. Turning off never touches credentials, so
 * it is a switch rather than a way to lose a sign-in.
 *
 * **Scope.** This is the one global setting behind everything GitWyrm writes
 * for you: commit messages, spec drafts, conflict help, and Agent Desk's own
 * "send this result back to the spec" (`ResultReviewPanel`, which runs on
 * exactly this provider). It is NOT what an Agent Desk chat sends to -- each
 * chat picks its own backend beside its message box, and nothing on that path
 * reads this switch.
 *
 * That distinction has to be visible, not merely true. This chip used to
 * describe itself as the trust anchor for every AI action in the window,
 * while a chat beneath it could be running on a different provider entirely,
 * or running at all with the chip reading "off". `scope="writing"` is what
 * Agent Desk passes to say which of the two questions this answers.
 */
export function AiProviderChip({
  repoId,
  scope = 'all',
}: {
  repoId: string | null
  /**
   * `all` -- this window has one AI and the chip speaks for it (Spec Desk).
   * `writing` -- this window also has per-chat AI, so the chip names only
   * what GitWyrm writes for you and says so.
   */
  scope?: AiChipScope
}) {
  const ai = useSpecAi()
  const run = useAiRun(repoId)
  const aiEnabled = useWorkspaceStore((s) => s.aiEnabled)
  const setAiEnabled = useWorkspaceStore((s) => s.setAiEnabled)

  // Switching off does not cancel work already underway -- stopping a run is the
  // console's job. Saying plainly that it is still going beats a chip that reads
  // "off" while the steps keep scrolling.
  const finishing = !aiEnabled && isActive(run.state)

  // A menu only earns its place once there is a choice to make. With one
  // provider the click is the whole interaction.
  const multi = ai.providers.length > 1

  // In a window that also has per-chat AI, the chip has to say which of the
  // two it means -- otherwise it reads as the answer to "what will my chat
  // send to?", which it is not.
  const writingScoped = scope === 'writing'
  const noun = aiChipNoun(scope)
  // Appended to every tooltip in this window, so the boundary is stated
  // wherever someone stops to ask what the chip governs.
  const scopeNote = aiChipScopeNote(scope)

  if (ai.providers.length === 0) {
    return (
      <TooltipHint label={`No AI is set up for writing. Click to add one.${scopeNote}`}>
        <ChipButton
          state={ai.state}
          label={`${noun} · not set up`}
          onClick={() => void openAiSettings()}
        />
      </TooltipHint>
    )
  }

  const label = finishing
    ? `${noun} · finishing run`
    : chipLabel(ai, aiEnabled, noun)

  if (!multi) {
    return (
      <TooltipHint
        label={
          aiEnabled
            ? `${writingScoped ? 'GitWyrm writes commit messages, spec drafts and conflict help with' : 'Runs use'} ${ai.provider}. Click to turn it off.${scopeNote}`
            : `${ai.providerShort} stays set up. Click to turn it back on.${scopeNote}`
        }
      >
        <ChipButton
          state={ai.state}
          label={label}
          onClick={() => {
            const next = !aiEnabled
            setAiEnabled(next)
            if (next) {
              toast.success(`${noun} is on. ${ai.providerShort} is ready.`)
            } else if (isActive(run.state)) {
              toast.info('AI is off. The run in progress will finish.', {
                description: `Stop it from the run console if you want it to end now. ${ai.providerShort} stays signed in.`,
              })
            } else {
              toast.info(`${noun} is off.`, {
                description: aiChipOffDescription(scope, ai.providerShort),
              })
            }
          }}
        />
      </TooltipHint>
    )
  }

  // The tooltip wraps the trigger rather than the other way round: whatever is
  // directly inside `DropdownMenuTrigger asChild` must be the button itself, or
  // the trigger's click handler never reaches it and the menu stops opening.
  return (
    <DropdownMenu>
      <TooltipHint
        label={`Choose which AI writes for you, or turn it off.${scopeNote}`}
      >
        <DropdownMenuTrigger asChild>
          <ChipButton state={ai.state} label={label} />
        </DropdownMenuTrigger>
      </TooltipHint>
      <DropdownMenuContent align="end" className="w-64">
        <ProviderItems writingScoped={writingScoped} />
        <DropdownMenuSeparator />
        <DropdownMenuItem
          onSelect={() => {
            setAiEnabled(false)
            toast.info(
              isActive(run.state) ? `${noun} is off. The run in progress will finish.` : `${noun} is off.`,
              { description: aiChipOffDescription(scope, ai.providerShort) }
            )
          }}
          disabled={!aiEnabled}
        >
          <Power />
          {`Turn ${noun.toLowerCase()} off`}
        </DropdownMenuItem>
        <DropdownMenuItem onSelect={() => void openAiSettings()}>
          <Settings2 />
          AI settings…
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

/**
 * One row per configured provider, with the model it would use.
 *
 * Picking a provider also turns the AI back on: choosing an AI is a clearer
 * statement of intent than the off switch it overrides, and leaving it off after
 * an explicit pick would read as the click being ignored.
 */
function ProviderItems({ writingScoped }: { writingScoped: boolean }) {
  const ai = useSpecAi()
  const catalog = useAiCatalog()
  const aiProvider = useWorkspaceStore((s) => s.aiProvider)
  const aiModels = useWorkspaceStore((s) => s.aiModels)
  const aiEnabled = useWorkspaceStore((s) => s.aiEnabled)
  const setDefaultAiProvider = useWorkspaceStore((s) => s.setDefaultAiProvider)
  const setAiEnabled = useWorkspaceStore((s) => s.setAiEnabled)

  const named = ai.providers.map((id) => ({
    id,
    name: (catalog.data ?? []).find((p) => p.id === id)?.name ?? id,
    model: aiModels[id] ?? null,
  }))

  return (
    <>
      <DropdownMenuLabel className="text-2xs text-sub">
        {writingScoped ? 'Use this for writing help' : 'Use this AI'}
      </DropdownMenuLabel>
      {named.map((p) => {
        const current = aiEnabled && p.id === aiProvider
        return (
          <DropdownMenuItem
            key={p.id}
            onSelect={() => {
              setDefaultAiProvider(p.id)
              setAiEnabled(true)
              toast.success(`Now using ${p.name}.`)
            }}
          >
            {current ? <Check /> : <span className="size-4 flex-none" aria-hidden />}
            <ProviderGlyph id={p.id} size={14} />
            <span className="min-w-0 flex-1 truncate">{p.name}</span>
            {p.model && (
              <span className="ml-auto max-w-[120px] flex-none truncate text-2xs text-muted-foreground">
                {p.model}
              </span>
            )}
          </DropdownMenuItem>
        )
      })}
    </>
  )
}

function chipLabel(
  ai: ReturnType<typeof useSpecAi>,
  enabled: boolean,
  noun: string
): string {
  if (!enabled) return `${noun} · off`
  if (ai.state === 'reconnect') return `${ai.providerShort} · reconnect`
  if (ai.state === 'ready') return `${ai.provider}${ai.model ? ` · ${ai.model}` : ''}`
  return ai.provider || `${noun} · not set up`
}

interface ChipButtonProps extends ComponentPropsWithoutRef<'button'> {
  state: ReturnType<typeof useSpecAi>['state']
  label: string
}

/**
 * The chip's looks, shared by the button and the menu trigger so both spellings
 * are the same object to the user.
 *
 * forwardRef and the prop spread are load-bearing: as a `DropdownMenuTrigger`
 * or `TooltipTrigger` `asChild` target this receives its onClick and ref by
 * merging, and a plain function component would drop both without any error --
 * the chip would simply stop opening the menu.
 *
 * For the same reason this renders a bare `<button>` and nothing else. Wrapping
 * it in a tooltip *here* would make that wrapper the asChild target, and the
 * trigger's handlers would land on the wrapper instead of the button. The
 * tooltip therefore goes on the outside, at each call site.
 */
const ChipButton = forwardRef<HTMLButtonElement, ChipButtonProps>(function ChipButton(
  { state, label, ...props },
  ref
) {
  return (
    <button
      ref={ref}
      type="button"
      className={cn(
        'titlebar-no-drag flex h-6 flex-none items-center gap-1.5 rounded-full border px-2.5 text-2xs font-semibold transition-colors',
        state === 'ready' && 'border-primary/35 bg-soft text-accent-text hover:border-primary',
        state === 'reconnect' &&
          'border-[var(--gw-amber)]/45 bg-[var(--gw-amber)]/10 text-[var(--gw-amber)] hover:border-[var(--gw-amber)]',
        (state === 'none' || state === 'off') &&
          'border-border text-muted-foreground hover:text-sub'
      )}
      {...props}
    >
      <span
        aria-hidden
        className={cn(
          'size-1.5 flex-none rounded-full',
          state === 'ready' && 'bg-primary',
          state === 'reconnect' && 'bg-[var(--gw-amber)]',
          (state === 'none' || state === 'off') && 'bg-muted-foreground'
        )}
      />
      {state === 'ready' && <Sparkles size={10} strokeWidth={2.4} className="flex-none" />}
      <span className="max-w-[220px] overflow-hidden text-ellipsis whitespace-nowrap">
        {label}
      </span>
    </button>
  )
})
