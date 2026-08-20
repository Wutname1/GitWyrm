import { Check, TriangleAlert } from 'lucide-react'
import type { ClientSyncState } from '@/lib/bindings'
import { syncBadgeClass, syncBadgeLabel } from '@/lib/agentConfig'
import { cn } from '@/lib/utils'

/**
 * One per-client cell in the sync table (mockup: `.ag-sync-state
 * good|warn|missing`). Every observed mockup badge text maps to exactly one
 * [`ClientSyncState`] variant -- see the doc comment on that enum in
 * `src-tauri/src/agent_config/model.rs` for the full mapping and how the
 * extra states beyond same/different/missing/unsupported/conflict were
 * reconciled.
 */
export function SyncStateBadge({ state }: { state: ClientSyncState }) {
  const cls = syncBadgeClass(state)
  const label = syncBadgeLabel(state)
  return (
    <span
      className={cn(
        'inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-2xs font-medium',
        cls === 'good' && 'bg-[color-mix(in_srgb,var(--gw-green)_16%,transparent)] text-[color-mix(in_srgb,var(--gw-green)_85%,var(--gw-text))]',
        cls === 'warn' && 'bg-[color-mix(in_srgb,var(--gw-amber)_16%,transparent)] text-[color-mix(in_srgb,var(--gw-amber)_85%,var(--gw-text))]',
        cls === 'missing' && 'text-muted-foreground'
      )}
    >
      {cls === 'good' && state !== 'keptSeparate' && <Check size={11} aria-hidden />}
      {cls === 'warn' && <TriangleAlert size={11} aria-hidden />}
      {label}
    </span>
  )
}
