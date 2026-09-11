import { useQuery } from '@tanstack/react-query'
import { Brain, Check, ChevronUp, Cpu } from 'lucide-react'
import { commands } from '@/lib/bindings'
import { keys, unwrap } from '@/lib/queryKeys'
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from '@/components/ui/dropdown-menu'
import { cn } from '@/lib/utils'

/**
 * Plain words for a thinking level, which the tools spell as bare adjectives.
 *
 * "xhigh" and "max" are not English. The ids stay exactly as the tool spells
 * them -- that is what goes on its command line -- and only the label changes.
 */
const EFFORT_LABELS: Record<string, string> = {
  none: 'No thinking',
  minimal: 'Barely think',
  low: 'Think a little',
  medium: 'Think it over',
  high: 'Think hard',
  xhigh: 'Think very hard',
  max: 'Think as hard as possible',
}

export function effortLabel(level: string): string {
  return EFFORT_LABELS[level] ?? level
}

/**
 * Which model this chat asks for, and how hard it asks the tool to think.
 *
 * Two chips rather than one control, because they are independent: a tool can
 * offer models and no thinking level, and the levels are the tool's own list
 * rather than a property of the model. Either chip is absent entirely when the
 * chosen tool cannot be told -- an empty menu implying a choice that does not
 * exist is worse than no chip.
 *
 * `null` means "whatever the tool is set up to use", and is deliberately not
 * resolved to the tool's current default: a default can change, and a chat
 * that never expressed a preference should keep following it rather than pin
 * the value it happened to have the day it started.
 */
export function ModelControl({
  sessionId,
  provider,
  model,
  onModelChange,
  effort,
  onEffortChange,
}: {
  sessionId: string | null
  /** The tool this chat runs, or `null` for the default one. */
  provider: string | null
  model: string | null
  onModelChange: (model: string | null) => void
  effort: string | null
  onEffortChange: (effort: string | null) => void
}) {
  const query = useQuery({
    queryKey: keys.agentProviders(sessionId),
    queryFn: async () => unwrap(await commands.agentProvidersList(sessionId)),
  })

  const rows = query.data?.providers ?? []
  // Which row this chat actually runs: the chosen tool, or the default one.
  const active = rows.find((r) => r.id === provider) ?? rows.find((r) => r.isDefault)
  const models = active?.models ?? []
  const levels = active?.effortLevels ?? []

  const modelLabel = models.find((m) => m.id === model)?.displayName ?? 'Model'
  const thinkingLabel = effort ? effortLabel(effort) : 'Thinking'

  return (
    <>
      {models.length > 0 && (
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <button
              type="button"
              title="Which model this chat asks for"
              className="flex flex-none items-center gap-1 rounded px-1.5 py-0.5 text-2xs font-semibold text-sub hover:bg-panel3 hover:text-foreground"
            >
              <Cpu size={12} aria-hidden />
              <span className="max-w-[10rem] truncate">{modelLabel}</span>
              <ChevronUp size={11} aria-hidden />
            </button>
          </DropdownMenuTrigger>
          <DropdownMenuContent side="top" align="start" className="w-[min(20rem,calc(100vw-3rem))]">
            {/* Explicitly offered, not just the absence of a choice: following
                the tool's own setting is a real answer, and the only one that
                keeps working when that setting changes. */}
            <DropdownMenuItem onSelect={() => onModelChange(null)}>
              <span className="min-w-0 flex-1 truncate">Whatever the tool uses</span>
              {model === null && <Check size={12} className="flex-none text-accent-text" aria-hidden />}
            </DropdownMenuItem>
            {models.map((m) => (
              <DropdownMenuItem key={m.id} onSelect={() => onModelChange(m.id)}>
                <span className="min-w-0 flex-1 truncate">{m.displayName}</span>
                {model === m.id && <Check size={12} className="flex-none text-accent-text" aria-hidden />}
              </DropdownMenuItem>
            ))}
          </DropdownMenuContent>
        </DropdownMenu>
      )}

      {levels.length > 0 && (
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <button
              type="button"
              title="How hard this chat asks the tool to think before answering"
              className={cn(
                'flex flex-none items-center gap-1 rounded px-1.5 py-0.5 text-2xs font-semibold',
                'text-sub hover:bg-panel3 hover:text-foreground'
              )}
            >
              <Brain size={12} aria-hidden />
              <span className="max-w-[10rem] truncate">{thinkingLabel}</span>
              <ChevronUp size={11} aria-hidden />
            </button>
          </DropdownMenuTrigger>
          <DropdownMenuContent side="top" align="start" className="w-[min(20rem,calc(100vw-3rem))]">
            <DropdownMenuItem onSelect={() => onEffortChange(null)}>
              <span className="min-w-0 flex-1 truncate">Whatever the tool uses</span>
              {effort === null && <Check size={12} className="flex-none text-accent-text" aria-hidden />}
            </DropdownMenuItem>
            {levels.map((level) => (
              <DropdownMenuItem key={level} onSelect={() => onEffortChange(level)}>
                <span className="min-w-0 flex-1 truncate">{effortLabel(level)}</span>
                {effort === level && <Check size={12} className="flex-none text-accent-text" aria-hidden />}
              </DropdownMenuItem>
            ))}
          </DropdownMenuContent>
        </DropdownMenu>
      )}
    </>
  )
}
