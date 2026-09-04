import { Brain, SearchCheck } from 'lucide-react'
import { cn } from '@/lib/utils'

/**
 * The lead's (or reviewer's) reasoning aside, rendered inline above the
 * message it belongs to -- matching the mockup's `.ag-thought`: a left
 * border, a tinted background, an icon, a bold lead word, then muted
 * reasoning text.
 *
 * `kind: 'thoughtSummary'` already exists on `SessionMessage` (see
 * `bindings.ts`), but `ConversationPane`'s old `kindLabel` mapped it to a
 * plain text label with no distinct visual treatment -- the same row style
 * as every other kind. This component is that missing treatment, mounted
 * for a `thoughtSummary` message that immediately precedes the reply it
 * belongs to (see `ConversationPane`'s grouping).
 *
 * `variant` picks the mockup's two observed lead words: a working lead
 * ("Thinking", brain icon, `--gw-purple`) and a read-only reviewer
 * ("Reviewing", search-check icon, same tint) -- both use the same purple
 * treatment in the mockup, only the icon and lead word differ.
 */
export interface ThoughtBlockProps {
  text: string
  variant?: 'thinking' | 'reviewing'
}

export function ThoughtBlock({ text, variant = 'thinking' }: ThoughtBlockProps) {
  const Icon = variant === 'reviewing' ? SearchCheck : Brain
  const label = variant === 'reviewing' ? 'Reviewing' : 'Thinking'
  return (
    <div
      className={cn(
        'mb-1.5 mt-0.5 flex items-start gap-2 rounded-sm border-l-2 px-2 py-1.5 text-2xs leading-relaxed',
        'border-l-[var(--gw-purple)] text-sub'
      )}
      style={{ background: 'color-mix(in srgb, var(--gw-purple) 7%, transparent)' }}
    >
      <Icon size={13} className="mt-0.5 flex-none text-[var(--gw-purple)]" aria-hidden />
      <p className="min-w-0">
        <strong className="font-semibold text-[var(--gw-purple)]">{label}</strong> {text}
      </p>
    </div>
  )
}
