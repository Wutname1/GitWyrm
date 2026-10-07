import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Brain, Check, ChevronUp, Cpu } from 'lucide-react'
import { commands, type AgentModelChoice } from '@/lib/bindings'
import { keys, unwrap } from '@/lib/queryKeys'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { HoverCard, HoverCardContent, HoverCardTrigger } from '@/components/ui/hover-card'
import { cn } from '@/lib/utils'
import { formatCredits, formatMultiplier, formatTokenCount, formatUsedPercent, fullestWindow } from '@/lib/providerUsage'
import { ProviderUsageBlock, UsageRing, useUsageFor } from './ProviderUsage'

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
 * One chip opening one menu, with a submenu for each choice. The two are
 * still independent -- a tool can offer models and no thinking level -- so a
 * submenu is left out when the tool cannot be told, and the whole chip when
 * it can be told neither: an empty menu implying a choice that does not exist
 * is worse than no chip.
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

  // The tool's plan limits: a ring on the chip once half of one is used, and
  // the full picture beside each model in the menu.
  const usage = useUsageFor(active?.id)
  const fullest = fullestWindow(usage)
  const chosen = models.find((m) => m.id === model) ?? null
  // The chosen model's own thinking levels when it reports them (Codex,
  // Copilot), otherwise the tool-wide list.
  const effortLevels = chosen && chosen.efforts.length > 0 ? chosen.efforts : levels
  const [search, setSearch] = useState('')
  const q = search.trim().toLowerCase()
  const shownModels = q ? models.filter((m) => `${m.displayName} ${m.id}`.toLowerCase().includes(q)) : models
  // Name each model's maker only when the list mixes makers, as Copilot's
  // does; on a one-maker tool it would repeat the same word on every row.
  const showVendor = new Set(models.map((m) => modelVendor(m.id))).size > 1

  if (models.length === 0 && effortLevels.length === 0) return null

  // One chip for the whole choice, like "Opus 5 · Think hard": the model and
  // how hard it thinks are picked together far more often than apart.
  const chipParts = [chosen?.displayName ?? (models.length > 0 ? 'Default model' : null)]
  if (effortLevels.length > 0) chipParts.push(effort ? effortLabel(effort) : null)
  const chipLabel = chipParts.filter(Boolean).join(' · ') || 'Model'

  return (
    <DropdownMenu onOpenChange={(open) => !open && setSearch('')}>
      <DropdownMenuTrigger asChild>
        <button
          type="button"
          title="Which model this chat uses, and how hard it thinks"
          className="flex min-w-0 flex-none items-center gap-1 rounded px-1.5 py-0.5 text-2xs font-semibold text-sub hover:bg-panel3 hover:text-foreground"
        >
          <Cpu size={12} aria-hidden />
          <span className="max-w-[14rem] truncate">{chipLabel}</span>
          {fullest && fullest.usedPercent >= 50 && (
            <span
              className="flex"
              aria-label={`${usage?.displayName ?? 'This tool'}: ${fullest.label.toLowerCase()} ${formatUsedPercent(fullest.usedPercent)}`}
            >
              <UsageRing percent={fullest.usedPercent} size={12} />
            </span>
          )}
          <ChevronUp size={11} aria-hidden />
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent side="top" align="start" className="w-64">
        {models.length > 0 && (
          <DropdownMenuSub>
            <DropdownMenuSubTrigger className="text-xs">
              <Cpu aria-hidden />
              Model
              <span className="ml-auto max-w-[8rem] truncate pl-3 text-2xs text-muted-foreground">
                {chosen?.displayName ?? 'Default'}
              </span>
            </DropdownMenuSubTrigger>
            <DropdownMenuSubContent className="w-64">
              {models.length > 8 && (
                <div className="p-1 pb-1.5">
                  <input
                    value={search}
                    onChange={(e) => setSearch(e.target.value)}
                    // The menu reads letters as "jump to item"; typing here
                    // must stay in the box.
                    onKeyDown={(e) => e.stopPropagation()}
                    placeholder="Search models"
                    aria-label="Search models"
                    className="w-full rounded border border-border bg-panel2 px-2 py-1 text-xs text-foreground outline-none placeholder:text-muted-foreground focus:border-primary"
                  />
                </div>
              )}
              {/* Following the tool's own setting is a real answer, and the
                  only one that keeps working when that setting changes. */}
              {!q && (
                <DropdownMenuItem onSelect={() => onModelChange(null)}>
                  <span className="min-w-0 flex-1 truncate">Whatever the tool uses</span>
                  {model === null && <Check size={12} className="flex-none text-accent-text" aria-hidden />}
                </DropdownMenuItem>
              )}
              {shownModels.length === 0 && (
                <p className="px-2 py-1.5 text-2xs text-muted-foreground">No models match "{search.trim()}".</p>
              )}
              {shownModels.map((m) => (
                <HoverCard key={m.id} openDelay={200} closeDelay={80}>
                  <HoverCardTrigger asChild>
                    <DropdownMenuItem onSelect={() => onModelChange(m.id)}>
                      <span className="min-w-0 flex-1 truncate">
                        {m.displayName}
                        {showVendor && modelVendor(m.id) && (
                          <span className="font-normal text-muted-foreground"> ({modelVendor(m.id)})</span>
                        )}
                      </span>
                      {m.multiplier != null && (
                        <span className="flex-none text-2xs text-muted-foreground">{formatMultiplier(m.multiplier)}</span>
                      )}
                      {model === m.id && <Check size={12} className="flex-none text-accent-text" aria-hidden />}
                    </DropdownMenuItem>
                  </HoverCardTrigger>
                  <HoverCardContent side="right" align="start" className="w-72 p-3">
                    <ModelDetails model={m} />
                    {usage && (
                      <div className="mt-3 border-t border-border pt-3">
                        <ProviderUsageBlock usage={usage} />
                      </div>
                    )}
                  </HoverCardContent>
                </HoverCard>
              ))}
            </DropdownMenuSubContent>
          </DropdownMenuSub>
        )}

        {effortLevels.length > 0 && (
          <DropdownMenuSub>
            <DropdownMenuSubTrigger className="text-xs">
              <Brain aria-hidden />
              Thinking
              <span className="ml-auto max-w-[8rem] truncate pl-3 text-2xs text-muted-foreground">
                {effort ? effortLabel(effort) : 'Default'}
              </span>
            </DropdownMenuSubTrigger>
            <DropdownMenuSubContent className="w-56">
              <DropdownMenuItem onSelect={() => onEffortChange(null)}>
                <span className="min-w-0 flex-1 truncate">Whatever the tool uses</span>
                {effort === null && <Check size={12} className="flex-none text-accent-text" aria-hidden />}
              </DropdownMenuItem>
              {effortLevels.map((level) => (
                <DropdownMenuItem key={level} onSelect={() => onEffortChange(level)}>
                  <span className="min-w-0 flex-1 truncate">{effortLabel(level)}</span>
                  {effort === level && <Check size={12} className="flex-none text-accent-text" aria-hidden />}
                </DropdownMenuItem>
              ))}
            </DropdownMenuSubContent>
          </DropdownMenuSub>
        )}

        {usage && fullest && (
          <>
            <DropdownMenuSeparator />
            <div className="px-2 py-1.5">
              <ProviderUsageBlock usage={usage} />
            </div>
          </>
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

/**
 * Who makes a model, from its id, for the "(Anthropic)" after its name. Only
 * the makers whose ids are unmistakable; anything else shows no maker rather
 * than a guess.
 */
export function modelVendor(id: string): string | null {
  const lower = id.toLowerCase()
  if (/^(claude|opus|sonnet|haiku|fable)/.test(lower)) return 'Anthropic'
  if (/^(gpt|o\d|codex|chatgpt)/.test(lower)) return 'OpenAI'
  if (lower.startsWith('gemini')) return 'Google'
  if (lower.startsWith('grok')) return 'xAI'
  return null
}

/**
 * What one model offers and costs, for the card beside its row: the facts a
 * tool's own picker shows, and only the ones this tool actually reported.
 */
function ModelDetails({ model }: { model: AgentModelChoice }) {
  const rows: [string, string][] = []
  if (model.contextWindow != null) rows.push(['Context', formatTokenCount(model.contextWindow)])
  if (model.efforts.length > 0) {
    rows.push(['Thinking', `${model.efforts.length} level${model.efforts.length === 1 ? '' : 's'}`])
  }
  if (model.multiplier != null) rows.push(['Premium requests', formatMultiplier(model.multiplier)])
  const credits = model.creditsPerMillion
  return (
    <div className="flex flex-col gap-2">
      <div>
        <p className="text-xs font-semibold text-foreground">{model.displayName}</p>
        {model.description && <p className="mt-0.5 text-2xs leading-relaxed text-muted-foreground">{model.description}</p>}
      </div>
      {rows.length > 0 && (
        <dl className="flex flex-col gap-0.5 border-t border-border pt-2">
          {rows.map(([label, value]) => (
            <div key={label} className="flex justify-between gap-2 text-2xs">
              <dt className="text-muted-foreground">{label}</dt>
              <dd className="font-medium text-foreground">{value}</dd>
            </div>
          ))}
        </dl>
      )}
      {credits && (
        <div className="border-t border-border pt-2">
          <p className="mb-1 text-2xs font-semibold text-foreground">AI credits per 1M tokens</p>
          <dl className="flex flex-col gap-0.5">
            {(
              [
                ['Input', credits.input],
                ['Cached input', credits.cachedInput],
                ['Output', credits.output],
              ] as [string, number | null][]
            )
              .filter((row): row is [string, number] => row[1] != null)
              .map(([label, value]) => (
                <div key={label} className="flex justify-between gap-2 text-2xs">
                  <dt className="text-muted-foreground">{label}</dt>
                  <dd className="font-medium text-foreground">{formatCredits(value)}</dd>
                </div>
              ))}
          </dl>
        </div>
      )}
    </div>
  )
}
