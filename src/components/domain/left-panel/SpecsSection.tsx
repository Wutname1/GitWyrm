import { ChevronRight, ExternalLink } from 'lucide-react'
import { cn } from '@/lib/utils'
import type { SpecChange } from '@/lib/bindings'
import { progressCount } from '@/lib/specDisplay'
import { useUiStore } from '@/stores/uiStore'

/**
 * A change's progress as a thin bar. Its own component because the generic
 * sidebar row has no room for one -- rows elsewhere are a single line of text.
 */
function ProgressBar({ percent }: { percent: number }) {
  return (
    <span
      className="mt-1 block h-[3px] overflow-hidden rounded-full bg-panel3"
      role="presentation"
    >
      <span
        className="block h-full rounded-full bg-primary transition-[width] duration-300"
        style={{ width: `${percent}%` }}
      />
    </span>
  )
}

function SpecRow({
  change,
  isSelected,
  onClick,
}: {
  change: SpecChange
  isSelected: boolean
  onClick: () => void
}) {
  const count = progressCount(change)
  return (
    <button
      type="button"
      onClick={onClick}
      aria-current={isSelected || undefined}
      className={cn(
        'block w-full border-l-2 py-1.5 pl-[22px] pr-3 text-left transition-colors hover:bg-panel2',
        isSelected ? 'border-primary bg-soft' : 'border-transparent'
      )}
    >
      <span className="flex items-center gap-2">
        <span
          className={cn(
            'flex-1 overflow-hidden text-ellipsis whitespace-nowrap font-mono text-xs',
            isSelected ? 'font-semibold text-foreground' : 'text-sub'
          )}
        >
          {change.id}
        </span>
        <span
          // A count is quiet data; "draft" is a state, so it reads as a word.
          className={cn(
            'flex-none font-mono text-2xs',
            change.progress.is_draft ? 'uppercase tracking-wide text-muted-foreground' : 'text-muted-foreground'
          )}
        >
          {count}
        </span>
      </span>
      <ProgressBar percent={change.progress.percent} />
    </button>
  )
}

/**
 * The Specs section of the left panel: one row per active OpenSpec change, with
 * live progress from its tasks.md.
 *
 * Not built on `SidebarSection` on purpose -- rows here carry a progress bar and
 * a second line, which the shared single-line row cannot express. The header
 * matches it visually so the panel still reads as one list.
 */
export function SpecsSection({
  changes,
  onOpenDesk,
}: {
  changes: SpecChange[]
  onOpenDesk: () => void
}) {
  const open = useUiStore((s) => s.sectionOpen.specs)
  const toggleSection = useUiStore((s) => s.toggleSection)
  const selectedId = useUiStore((s) => s.selectedChangeId)
  const selectChange = useUiStore((s) => s.selectChange)

  // Mirrors useSelectedChange: with nothing clicked, the first change is the
  // one the card is showing, so it is the one that looks selected here.
  const effectiveId = changes.find((c) => c.id === selectedId)?.id ?? changes[0]?.id

  return (
    <div className="group/section">
      <div
        onClick={() => toggleSection('specs')}
        className="flex cursor-pointer select-none items-center gap-1.5 py-1.5 pl-2.5 pr-3 hover:bg-panel2"
      >
        <ChevronRight
          size={12}
          strokeWidth={2.4}
          className={cn(
            'flex-none text-muted-foreground transition-transform duration-100',
            open && 'rotate-90'
          )}
        />
        <span className="text-2xs font-bold tracking-[.09em] text-sub">SPECS</span>
        <span className="ml-auto font-mono text-2xs text-muted-foreground">{changes.length}</span>
      </div>
      {open && (
        <div className="pb-1">
          {changes.length === 0 ? (
            <p className="px-6 py-1 text-2xs text-muted-foreground">
              No active changes in openspec/changes.
            </p>
          ) : (
            changes.map((change) => (
              <SpecRow
                key={change.id}
                change={change}
                isSelected={change.id === effectiveId}
                onClick={() => selectChange(change.id)}
              />
            ))
          )}
          <button
            type="button"
            onClick={onOpenDesk}
            className="mt-0.5 flex w-full items-center gap-1.5 py-1 pl-[22px] pr-3 text-left text-2xs text-muted-foreground hover:text-accent-text"
          >
            <ExternalLink size={11} strokeWidth={2} className="flex-none" />
            Open Spec Desk
          </button>
        </div>
      )}
    </div>
  )
}
