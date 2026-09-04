import type { ClientId, ClientSyncStatus, InventoryEntry, ItemKind, ClientSyncState, UndoOutcome } from '@/lib/bindings'

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

/**
 * Every client column, in the fixed order the table shows them.
 *
 * Derived from `CLIENT_LABEL` rather than hand-listed. A hand-written list
 * silently dropped `open-chamber`, which has a real writer in the backend
 * registry: it was pre-ticked by `eligibleDestinationsFor` (which reads the
 * row's own per-client states, not this order), had no checkbox to untick,
 * and had no column showing its state -- so a copy could be written to it
 * without ever being offered. Keying off the label map means adding a client
 * cannot repeat that: a new key appears here the moment it has a name.
 */
export const CLIENT_COLUMN_ORDER: ClientId[] = Object.keys(CLIENT_LABEL) as ClientId[]

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

/**
 * Plain-language explanation of an Undo outcome, with whether it actually
 * put anything back.
 *
 * Exists because the undo mutation used to resolve this value and throw it
 * away, so pressing Undo looked identical whether the file was restored or
 * deliberately left alone. The most important case is
 * `concurrentChangeRefused`: the file changed after GitWyrm wrote it, so
 * undoing would clobber a newer edit -- nothing was touched, and the person
 * needs to know their file is NOT back to how it was.
 *
 * Mirrors `explainUndoOutcome` in `agentDeskResult.ts`, which does the same
 * job for an agent run's own undo.
 */
export function explainConfigUndoOutcome(outcome: UndoOutcome): { message: string; restored: boolean } {
  switch (outcome.kind) {
    case 'restored':
      return { message: 'Put back. The file is how it was before the copy.', restored: true }
    case 'alreadyUndone':
      return { message: 'That copy was already put back, so nothing changed.', restored: false }
    case 'operationNotFound':
      return { message: 'That copy could not be found, so nothing was changed.', restored: false }
    case 'concurrentChangeRefused':
      return {
        message: 'Left alone -- the file changed after GitWyrm copied to it, and putting it back would undo that newer change.',
        restored: false,
      }
    case 'restoreFailed':
      return { message: `Could not put it back: ${outcome.detail}`, restored: false }
  }
}

/**
 * One line describing a config change GitWyrm made, for the recent-changes
 * list in Agent Setup.
 *
 * Says which app's settings were touched and whether the file existed before,
 * because "changed a file you already had" and "created a file that was not
 * there" are different things to undo. The path is shown separately by the
 * caller; this is the sentence above it.
 */
export function describeConfigOperation(receipt: {
  client: string
  beforeHash: string | null
  undone: boolean
}): string {
  const what = receipt.beforeHash === null ? 'Created settings for' : 'Changed settings for'
  return receipt.undone ? `${what} ${receipt.client} (already put back)` : `${what} ${receipt.client}`
}

/**
 * How many items match, differ, or live in only one app.
 *
 * The three used to be computed with overlapping predicates, so they could add
 * up to more than the total: an item present in exactly one app satisfied both
 * "match" (all its states are `isSource`/`clientNotDetected`) and "exists in
 * one app". Three numbers printed beside a total are read as parts of it, so
 * they are mutually exclusive and evaluated in priority order: a differing
 * item is reported as differing whatever else is true of it.
 */
export function summarizeInventoryCounts(rows: InventoryEntry[]): {
  total: number
  matching: number
  differing: number
  existsInOne: number
} {
  const isDiffering = (e: InventoryEntry) => e.perClient.some((s) => s.state === 'different' || s.state === 'outdated')
  const isOnlyInOne = (e: InventoryEntry) =>
    e.perClient.every(
      (s) => s.state === 'isSource' || s.state === 'missing' || s.state === 'clientNotDetected' || s.state === 'unsupported'
    )
  const differing = rows.filter(isDiffering).length
  const existsInOne = rows.filter((e) => !isDiffering(e) && isOnlyInOne(e)).length
  const matching = rows.filter(
    (e) =>
      !isDiffering(e) &&
      !isOnlyInOne(e) &&
      e.perClient.every(
        (s) => s.state === 'same' || s.state === 'isSource' || s.state === 'keptSeparate' || s.state === 'clientNotDetected'
      )
  ).length
  return { total: rows.length, matching, differing, existsInOne }
}

/**
 * The name a person recognises for an inventory item, given its internal id.
 *
 * `RedactedCopyPlan` carries only `itemId`, which is a namespaced key like
 * `McpConnector:github` -- so the batch review dialog headlined every card
 * with a code identifier while the single-item dialog right beside it used the
 * real name. Falls back to the id rather than to nothing: an unrecognisable
 * heading beats a blank one.
 */
export function itemDisplayName(entries: InventoryEntry[], itemId: string): string {
  return entries.find((e) => e.itemId === itemId)?.displayName ?? itemId
}

/**
 * Why a copy could not be previewed.
 *
 * Both refusals used to end at `setPlan(null)`, which returns the dialog to
 * the destination picker -- indistinguishable from not having chosen yet, so
 * the person picks the same destinations again and gets the same silence.
 */
export function explainPreviewRefusal(kind: 'itemNotFound' | 'noDestinations'): string {
  return kind === 'noDestinations'
    ? 'There is nowhere to copy this to. The other apps either already have it or cannot use it.'
    : 'That item is no longer there. It may have been changed or removed since the last scan.'
}
