import { forwardRef, type ComponentPropsWithoutRef } from 'react'
import { Check, Power, Settings2, Sparkles } from 'lucide-react'
import { toast } from 'sonner'
import { cn } from '@/lib/utils'
import { useAiCatalog } from '@/hooks/useAi'
import { useSpecAi } from '@/hooks/useSpecAi'
import { isActive, useAiRun } from '@/hooks/useAiRun'
import { openAiSettings } from '@/lib/openAiSettings'
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

/**
 * Which AI the Desk would use, and the switch for it, in the titlebar.
 *
 * The single trust anchor for every AI action in this window: whatever the rail
 * offers, this says who would do it. It is deliberately always present -- a
 * missing chip would leave "is an AI involved here?" unanswered.
 *
 * It is also the control. With one provider set up a click turns the AI off and
 * on; with several it opens a picker. Turning off never touches credentials, so
 * it is a switch rather than a way to lose a sign-in.
 */
export function AiProviderChip({ repoId }: { repoId: string | null }) {
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

  if (ai.providers.length === 0) {
    return (
      <TooltipHint label="No AI is set up. Click to add one.">
        <ChipButton
          state={ai.state}
          label="AI · not set up"
          onClick={() => void openAiSettings()}
        />
      </TooltipHint>
    )
  }

  const label = finishing ? 'AI · finishing run' : chipLabel(ai, aiEnabled)

  if (!multi) {
    return (
      <TooltipHint
        label={
          aiEnabled
            ? `Runs use ${ai.provider}. Click to turn the AI off.`
            : `${ai.providerShort} stays set up. Click to turn the AI back on.`
        }
      >
        <ChipButton
          state={ai.state}
          label={label}
          onClick={() => {
            const next = !aiEnabled
            setAiEnabled(next)
            if (next) {
              toast.success(`AI is on. ${ai.providerShort} is ready.`)
            } else if (isActive(run.state)) {
              toast.info('AI is off. The run in progress will finish.', {
                description: `Stop it from the run console if you want it to end now. ${ai.providerShort} stays signed in.`,
              })
            } else {
              toast.info('AI is off.', {
                description: `${ai.providerShort} stays signed in. Copying handoffs still works.`,
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
      <TooltipHint label="Choose which AI to use, or turn it off.">
        <DropdownMenuTrigger asChild>
          <ChipButton state={ai.state} label={label} />
        </DropdownMenuTrigger>
      </TooltipHint>
      <DropdownMenuContent align="end" className="w-64">
        <ProviderItems />
        <DropdownMenuSeparator />
        <DropdownMenuItem
          onSelect={() => {
            setAiEnabled(false)
            toast.info(
              isActive(run.state) ? 'AI is off. The run in progress will finish.' : 'AI is off.',
              {
                description: 'Your providers stay signed in. Copying handoffs still works.',
              }
            )
          }}
          disabled={!aiEnabled}
        >
          <Power />
          Turn AI off
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
function ProviderItems() {
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
      <DropdownMenuLabel className="text-2xs text-sub">Use this AI</DropdownMenuLabel>
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

function chipLabel(ai: ReturnType<typeof useSpecAi>, enabled: boolean): string {
  if (!enabled) return 'AI · off'
  if (ai.state === 'reconnect') return `${ai.providerShort} · reconnect`
  if (ai.state === 'ready') return `${ai.provider}${ai.model ? ` · ${ai.model}` : ''}`
  return ai.provider || 'AI · not set up'
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
