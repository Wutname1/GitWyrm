import { FilePenLine, SearchCheck, ShieldCheck, Wrench } from 'lucide-react'
import type { EventStackItem } from '@/lib/agentDeskEvents'
import { resolveMessageTarget } from '@/lib/agentDeskTargets'
import { DisabledHint } from '@/components/ui/tooltip'
import { cn } from '@/lib/utils'

/**
 * The compact activity feed indented under a message -- matching the
 * mockup's `.ag-event-stack`/`.ag-event`/`.ag-event-link`: a 17px icon, a
 * bold headline plus muted detail text, and a right-aligned link button
 * ("Output", "View diff", "Worktree").
 *
 * Items come from `groupEventStacks` (`src/lib/agentDeskEvents.ts`), which
 * groups runs of consecutive `tool`-kind messages -- there is no dedicated
 * `MessageKind` for "event" (see that file's doc comment for why).
 *
 * The link reuses `resolveMessageTarget` (the same mapping `ConversationPane`
 * already uses for a message's own target links), so an event link is
 * honestly disabled -- with a reason, via `DisabledHint` -- whenever Agent
 * Desk cannot actually reach the destination yet, never a dead-looking
 * button.
 */
export interface EventStackProps {
  items: EventStackItem[]
  onOpenSource?: () => void
}

/** Icon per event, chosen from the headline's own wording -- matching the mockup's per-row icon variety. */
function iconForHeadline(headline: string) {
  const lower = headline.toLowerCase()
  if (lower.includes('research') || lower.includes('review')) return SearchCheck
  if (lower.includes('untouched') || lower.includes('safe') || lower.includes('isolated')) return ShieldCheck
  if (lower.includes('changed') || lower.includes('file') || lower.includes('edit')) return FilePenLine
  return Wrench
}

function EventLink({ item, onOpenSource }: { item: EventStackItem; onOpenSource?: () => void }) {
  if (!item.target) return null
  const resolved = resolveMessageTarget(item.target)
  if (resolved.kind === 'source') {
    return (
      <button
        type="button"
        onClick={onOpenSource}
        disabled={!onOpenSource}
        className="flex-none text-[10.5px] font-medium text-accent-text hover:underline disabled:cursor-not-allowed disabled:opacity-50 disabled:no-underline"
      >
        {resolved.label}
      </button>
    )
  }
  return (
    <DisabledHint disabled reason={resolved.reason}>
      <button
        type="button"
        disabled
        className="flex-none max-w-[9rem] truncate text-[10.5px] font-medium text-muted-foreground disabled:cursor-not-allowed"
      >
        {resolved.label}
      </button>
    </DisabledHint>
  )
}

export function EventStack({ items, onOpenSource }: EventStackProps) {
  if (items.length === 0) return null
  return (
    <div aria-label="Recent agent activity" className="ml-9 mt-0.5 mb-2">
      {items.map((item, i) => {
        const Icon = iconForHeadline(item.headline)
        return (
          <div
            key={item.messageId}
            className={cn(
              'grid min-h-[30px] grid-cols-[17px_minmax(0,1fr)_auto] items-center gap-1.5 py-1 text-[11px] text-sub',
              i < items.length - 1 && 'border-b border-[color-mix(in_srgb,var(--gw-border)_65%,transparent)]'
            )}
          >
            <Icon size={13} className="text-muted-foreground" aria-hidden />
            <span className="min-w-0 truncate">
              <strong className="font-semibold text-foreground">{item.headline}</strong>
              {item.detail && <> · {item.detail}</>}
            </span>
            <EventLink item={item} onOpenSource={onOpenSource} />
          </div>
        )
      })}
    </div>
  )
}
