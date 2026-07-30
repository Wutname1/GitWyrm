import { useState } from 'react'
import { Archive, Plus } from 'lucide-react'
import { cn } from '@/lib/utils'
import type { SpecChange } from '@/lib/bindings'
import { formatRelativeTime } from '@/lib/gitDisplay'
import { progressCount, STATUS_LABEL, statusTone } from '@/lib/specDisplay'
import { selectChangeEverywhere } from '@/lib/specSync'
import { useUiStore } from '@/stores/uiStore'
import { toast } from 'sonner'

/** Which subset of changes the list is showing. */
type Filter = 'active' | 'review' | 'mine'

const FILTERS: Array<{ key: Filter; label: string }> = [
  { key: 'active', label: 'Active' },
  { key: 'review', label: 'Needs review' },
  { key: 'mine', label: 'Mine' },
]

const TONE_CLASS = {
  muted: 'border-muted-foreground/40 text-muted-foreground',
  info: 'border-[var(--gw-blue)]/40 text-[var(--gw-blue)] bg-[var(--gw-blue)]/8',
  warn: 'border-[var(--gw-amber)]/40 text-[var(--gw-amber)] bg-[var(--gw-amber)]/8',
  good: 'border-primary/40 text-accent-text bg-soft',
} as const

function matchesFilter(change: SpecChange, filter: Filter): boolean {
  switch (filter) {
    case 'review':
      return change.status === 'needsReview'
    case 'mine':
      // Authorship is not in the files, so "Mine" cannot be answered honestly
      // yet. Until commit-based attribution lands it shows everything rather
      // than guessing and quietly hiding a teammate's change.
      return true
    case 'active':
      return true
  }
}

function ChangeRow({
  change,
  isSelected,
  onSelect,
}: {
  change: SpecChange
  isSelected: boolean
  onSelect: () => void
}) {
  return (
    <button
      type="button"
      onClick={onSelect}
      aria-current={isSelected || undefined}
      className={cn(
        'relative block w-full rounded-lg border px-3 py-2.5 text-left transition-colors',
        isSelected
          ? 'border-primary/25 bg-soft'
          : 'border-transparent hover:bg-panel2'
      )}
    >
      {isSelected && (
        <span
          aria-hidden
          className="absolute inset-y-2.5 left-0 w-0.5 rounded-full bg-primary"
        />
      )}
      <span className="block overflow-hidden text-ellipsis whitespace-nowrap font-mono text-xs font-semibold text-foreground">
        {change.id}
      </span>
      <span className="mt-1.5 flex items-center gap-2">
        <span className="flex-1 overflow-hidden text-ellipsis whitespace-nowrap text-2xs text-muted-foreground">
          updated {formatRelativeTime(change.updated)}
        </span>
        <span
          className={cn(
            'flex-none rounded-full border px-1.5 py-px text-2xs font-semibold',
            TONE_CLASS[statusTone(change.status)]
          )}
        >
          {STATUS_LABEL[change.status]}
        </span>
      </span>
      <span className="mt-2 flex items-center gap-2">
        <span className="block h-[3px] flex-1 overflow-hidden rounded-full bg-panel3">
          <span
            className="block h-full rounded-full bg-primary transition-[width] duration-300"
            style={{ width: `${change.progress.percent}%` }}
          />
        </span>
        <span className="flex-none font-mono text-2xs text-muted-foreground">
          {progressCount(change)}
        </span>
      </span>
    </button>
  )
}

/**
 * The Desk's left column: every active change, filterable, with the archive
 * count underneath. Selecting a row selects that change in both windows.
 */
export function DeskChangesList({
  changes,
  archivedCount,
  selectedId,
}: {
  changes: SpecChange[]
  archivedCount: number
  selectedId: string | undefined
}) {
  const [filter, setFilter] = useState<Filter>('active')
  const rows = changes.filter((c) => matchesFilter(c, filter))
  const reviewCount = changes.filter((c) => c.status === 'needsReview').length

  return (
    <div className="flex min-h-0 flex-col border-r border-border bg-panel">
      <div className="flex flex-none items-center justify-between px-4 pb-2.5 pt-4">
        <h2 className="text-2xs font-bold tracking-[.1em] text-sub">CHANGES</h2>
        <button
          type="button"
          onClick={() =>
            toast.info('Creating changes from the Desk comes with the actions work.')
          }
          className="flex items-center gap-1 rounded border border-primary/40 bg-soft px-2 py-0.5 text-2xs font-bold text-accent-text hover:bg-primary/20"
        >
          <Plus size={10} strokeWidth={3} />
          New
        </button>
      </div>

      <div className="flex flex-none gap-3.5 border-b border-border px-4 pb-2.5">
        {FILTERS.map((f) => (
          <button
            key={f.key}
            type="button"
            onClick={() => setFilter(f.key)}
            aria-pressed={filter === f.key}
            className={cn(
              'border-b pb-0.5 text-2xs transition-colors',
              filter === f.key
                ? 'border-primary text-foreground'
                : 'border-transparent text-muted-foreground hover:text-sub'
            )}
          >
            {f.label}
            {f.key === 'review' && reviewCount > 0 && (
              <span className="ml-1 font-mono text-muted-foreground">{reviewCount}</span>
            )}
          </button>
        ))}
      </div>

      <div className="flex-1 overflow-y-auto p-2">
        {rows.length === 0 ? (
          <p className="px-2 py-3 text-2xs text-muted-foreground">
            {changes.length === 0
              ? 'No active changes in openspec/changes.'
              : 'Nothing matches this filter.'}
          </p>
        ) : (
          <div className="flex flex-col gap-0.5">
            {rows.map((change) => (
              <ChangeRow
                key={change.id}
                change={change}
                isSelected={change.id === selectedId}
                onSelect={() => selectChangeEverywhere(change.id)}
              />
            ))}
          </div>
        )}
      </div>

      <button
        type="button"
        onClick={() =>
          toast.info('Browsing archived changes comes with the actions work.')
        }
        className="mx-4 mb-3.5 flex flex-none items-center gap-2 border-t border-border pt-2.5 text-left text-2xs text-muted-foreground hover:text-foreground"
      >
        <Archive size={11} strokeWidth={2} className="flex-none" />
        Archive · {archivedCount} completed {archivedCount === 1 ? 'change' : 'changes'}
      </button>
    </div>
  )
}

/** Exported for the store-independent filter test in the Desk view. */
export { matchesFilter }
