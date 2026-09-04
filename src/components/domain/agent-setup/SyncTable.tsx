import type { InventoryEntry, ItemKind } from '@/lib/bindings'
import { CLIENT_COLUMN_ORDER, clientLabel, formatSummaryLine } from '@/lib/agentConfig'
import { SyncStateBadge } from './SyncStateBadge'

/**
 * The per-item inventory table (mockup `.ag-sync-table`): source, per-client
 * state, and scope, filtered to one item kind at a time (task 2.1). Clicking
 * a row opens the preview flow for that item (task 2.2: "Let the user choose
 * one item and one or more destinations").
 */
export function SyncTable({
  entries,
  kind,
  onSelectItem,
}: {
  entries: InventoryEntry[]
  kind: ItemKind
  onSelectItem: (entry: InventoryEntry) => void
}) {
  const rows = entries.filter((e) => e.kind === kind)
  const kindLabel = kind === 'skill' ? 'Skill' : 'Connector'

  if (rows.length === 0) {
    return (
      <p className="py-6 text-center text-2xs text-muted-foreground">
        No {kind === 'skill' ? 'skills' : 'MCP connectors'} were found on this machine yet.
      </p>
    )
  }

  return (
    <div className="w-full overflow-x-auto">
      <table className="w-full min-w-[690px] table-fixed border-collapse text-2xs">
        <thead>
          <tr>
            <th className="w-[31%] border-b border-border px-1.5 text-left font-semibold text-muted-foreground">
              {kindLabel}
            </th>
            {CLIENT_COLUMN_ORDER.map((client) => (
              <th
                key={client}
                className="border-b border-border px-1.5 text-center font-semibold text-muted-foreground"
              >
                {clientLabel(client)}
              </th>
            ))}
            <th className="border-b border-border px-1.5 text-center font-semibold text-muted-foreground">
              Scope
            </th>
          </tr>
        </thead>
        <tbody>
          {rows.map((entry) => (
            <tr
              key={entry.itemId}
              onClick={() => onSelectItem(entry)}
              className="cursor-pointer border-b border-border transition-colors hover:bg-panel2"
            >
              <td className="px-1.5 py-2 align-middle">
                <div className="flex flex-col gap-0.5">
                  <span className="truncate font-medium text-foreground">{entry.displayName}</span>
                  <span className="truncate text-2xs text-muted-foreground">
                    {entry.source.kind === 'repository'
                      ? 'source: this repository'
                      : `source: ${clientLabel(entry.source.client)}`}
                  </span>
                </div>
              </td>
              {CLIENT_COLUMN_ORDER.map((client) => {
                const status = entry.perClient.find((s) => s.client === client)
                return (
                  <td key={client} className="px-1.5 py-2 text-center align-middle">
                    {status && <SyncStateBadge state={status.state} />}
                  </td>
                )
              })}
              <td className="px-1.5 py-2 text-center align-middle text-muted-foreground">
                {entry.scope === 'repo' ? 'This repo' : 'Personal'}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}

/** The summary line above the table (mockup: "18 skills found - 11 match - 3 differ - 4 exist in one app"). */
export function SyncSummaryLine({
  entries,
  kind,
}: {
  entries: InventoryEntry[]
  kind: ItemKind
}) {
  const rows = entries.filter((e) => e.kind === kind)
  const matching = rows.filter((e) => e.perClient.every((s) => s.state === 'same' || s.state === 'isSource' || s.state === 'keptSeparate' || s.state === 'clientNotDetected')).length
  const differing = rows.filter((e) => e.perClient.some((s) => s.state === 'different' || s.state === 'outdated')).length
  const existsInOne = rows.filter((e) => e.perClient.every((s) => s.state === 'isSource' || s.state === 'missing' || s.state === 'clientNotDetected' || s.state === 'unsupported')).length

  const summary = { total: rows.length, matching, differing, existsInOne }

  return (
    <div className="mb-2.5 flex items-center gap-3 text-[10.5px] text-muted-foreground">
      <strong className="font-semibold text-foreground">{formatSummaryLine(summary, kind)}</strong>
      <span>{matching} match</span>
      <span>{differing} differ</span>
      <span>{existsInOne} exist in one app</span>
    </div>
  )
}
