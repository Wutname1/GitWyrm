import type { ClientId, ClientSyncStatus, InventoryEntry, ItemKind, ClientSyncState } from '@/lib/bindings'

/**
 * Aggregate counts for the setup view's summary line (mockup: "18 skills
 * found - 11 match - 3 differ - 4 exist in one app"). Computed entirely on
 * the frontend from [`InventoryEntry`] rows (see `SyncSummaryLine` in
 * `SyncTable.tsx`) rather than mirroring the Rust `InventorySummary` type --
 * that type has no command that returns it today, so specta does not export
 * it; defining the shape here keeps `formatSummaryLine` typed without
 * depending on a binding that does not exist yet.
 */
export interface InventorySummary {
  total: number
  matching: number
  differing: number
  existsInOne: number
}

/**
 * Agent Setup: pure helpers over the backend's inventory/sync-state shapes.
 *
 * Kept out of any component so the state -> badge-class/label mapping (the
 * ~9 states the mockup's `ag-sync-state good|warn|missing` badges show, see
 * `ClientSyncState` in `src-tauri/src/agent_config/model.rs`) is unit-testable
 * under vitest's NODE environment without a DOM.
 */

/** Badge class matching the mockup's `ag-sync-state good|warn|missing`. */
export type SyncBadgeClass = 'good' | 'warn' | 'missing'

const BADGE_CLASS: Record<ClientSyncState, SyncBadgeClass> = {
  same: 'good',
  isSource: 'good',
  keptSeparate: 'good',
  different: 'warn',
  outdated: 'warn',
  needsSetup: 'warn',
  missing: 'missing',
  unsupported: 'missing',
  disabled: 'missing',
  clientNotDetected: 'missing',
}

const BADGE_LABEL: Record<ClientSyncState, string> = {
  same: 'Synced',
  different: 'Different',
  missing: 'Missing',
  outdated: 'Older',
  isSource: 'Source',
  unsupported: 'Unsupported',
  keptSeparate: 'Own copy',
  needsSetup: 'Needs setup',
  disabled: 'Off',
  clientNotDetected: 'Not detected',
}

export function syncBadgeClass(state: ClientSyncState): SyncBadgeClass {
  return BADGE_CLASS[state]
}

export function syncBadgeLabel(state: ClientSyncState): string {
  return BADGE_LABEL[state]
}

export const CLIENT_LABEL: Record<ClientId, string> = {
  codex: 'Codex',
  'claude-code': 'Claude',
  'open-code': 'OpenCode',
  'vs-code-copilot': 'Copilot',
  'open-chamber': 'OpenChamber',
}

export function clientLabel(client: ClientId): string {
  return CLIENT_LABEL[client]
}

/** Every client column in the fixed order the mockup's table uses. */
export const CLIENT_COLUMN_ORDER: ClientId[] = ['codex', 'claude-code', 'open-code', 'vs-code-copilot']

/** Plain-language summary line: "18 skills found - 11 match - 3 differ - 4 exist in one app". */
export function formatSummaryLine(summary: InventorySummary, kind: ItemKind): string {
  const noun = kind === 'skill' ? 'skill' : 'connector'
  const plural = summary.total === 1 ? noun : `${noun}s`
  return `${summary.total} ${plural} found`
}

/** A destination is eligible to receive a copy: present in the row, not the source, and not already the same. */
export function isEligibleDestination(status: ClientSyncStatus): boolean {
  return (
    status.state !== 'isSource' &&
    status.state !== 'same' &&
    status.state !== 'keptSeparate' &&
    status.state !== 'clientNotDetected' &&
    // 'unsupported' means this client has no writer at all. Which clients
    // those are is the backend's registry to decide, not a list to repeat
    // here. Offering one as a destination would build a plan that can never
    // be applied, which is exactly the kind of button-that-cannot-work this
    // release is removing.
    status.state !== 'unsupported'
  )
}

/** Clients this row could still usefully be copied to, given its current per-client states. */
export function eligibleDestinationsFor(entry: InventoryEntry): ClientId[] {
  return entry.perClient.filter(isEligibleDestination).map((s) => s.client)
}

/** Whether an entry needs attention at all -- used to build the "Match selected apps" default selection. */
export function hasAnyDifference(entry: InventoryEntry): boolean {
  return entry.perClient.some((s) => s.state === 'different' || s.state === 'outdated' || s.state === 'missing')
}

/**
 * Which entries `BatchReviewDialog` ("Match selected apps") should attempt to
 * build a plan for, versus which have nothing eligible to copy to at all.
 * Extracted as a pure function so the partition itself -- not just its
 * downstream async preview calls -- is unit-testable without a DOM (R7.5:
 * the batch flow must build one plan per differing item with an eligible
 * destination, and skip the rest honestly rather than silently dropping
 * them).
 */
export function partitionBatchCandidates(entries: InventoryEntry[]): {
  candidates: { entry: InventoryEntry; destinations: ClientId[] }[]
  skipped: InventoryEntry[]
} {
  const candidates: { entry: InventoryEntry; destinations: ClientId[] }[] = []
  const skipped: InventoryEntry[] = []
  for (const entry of entries.filter(hasAnyDifference)) {
    const destinations = eligibleDestinationsFor(entry)
    if (destinations.length === 0) {
      skipped.push(entry)
    } else {
      candidates.push({ entry, destinations })
    }
  }
  return { candidates, skipped }
}
