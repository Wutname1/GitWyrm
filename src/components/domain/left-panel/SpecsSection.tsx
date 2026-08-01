import { forwardRef, type ComponentPropsWithoutRef } from 'react'
import { ChevronRight, ExternalLink, Plus, RefreshCw } from 'lucide-react'
import { useQueryClient } from '@tanstack/react-query'
import { cn } from '@/lib/utils'
import type { SpecChange } from '@/lib/bindings'
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from '@/components/ui/context-menu'
import { SpecContextMenu } from '@/components/domain/spec-desk/SpecContextMenu'
import { progressCount } from '@/lib/specDisplay'
import { invalidateOpenspec } from '@/lib/queryKeys'
import { selectChangeEverywhere } from '@/lib/specSync'
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

// forwardRef and the prop spread are load-bearing: `ContextMenuTrigger asChild`
// merges its onContextMenu handler and a ref onto this child. A plain component
// that only names its own props drops both, and the row silently stops answering
// a right-click.
const SpecRow = forwardRef<
  HTMLButtonElement,
  {
    change: SpecChange
    isSelected: boolean
    onClick: () => void
  } & ComponentPropsWithoutRef<'button'>
>(function SpecRow({ change, isSelected, onClick, className, ...props }, ref) {
  const count = progressCount(change)
  return (
    <button
      type="button"
      {...props}
      ref={ref}
      onClick={onClick}
      aria-current={isSelected || undefined}
      className={cn(
        'block w-full border-l-2 py-1.5 pl-[22px] pr-3 text-left transition-colors hover:bg-panel2',
        isSelected ? 'border-primary bg-soft' : 'border-transparent',
        className
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
})

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
  hasOpenspec,
  repoId,
  onOpenDesk,
  onNewChange,
}: {
  changes: SpecChange[]
  /** Whether the repository has an `openspec/` folder yet. */
  hasOpenspec: boolean
  repoId: string
  onOpenDesk: () => void
  onNewChange: () => void
}) {
  const open = useUiStore((s) => s.sectionOpen.specs)
  const toggleSection = useUiStore((s) => s.toggleSection)
  const selectedId = useUiStore((s) => s.selectedChangeId)
  const qc = useQueryClient()

  return (
    <div className="group/section">
      <ContextMenu>
        <ContextMenuTrigger asChild>
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
            <span className="ml-auto font-mono text-2xs text-muted-foreground">
              {changes.length}
            </span>
          </div>
        </ContextMenuTrigger>
        <ContextMenuContent className="w-56">
          <ContextMenuItem onSelect={onNewChange}>
            <Plus />
            Start a new change…
          </ContextMenuItem>
          <ContextMenuSeparator />
          <ContextMenuItem onSelect={() => invalidateOpenspec(qc, repoId)}>
            <RefreshCw />
            Re-read from disk
          </ContextMenuItem>
        </ContextMenuContent>
      </ContextMenu>
      {open && (
        <div className="pb-1">
          {changes.length === 0 ? (
            <div className="px-6 py-1">
              <p className="text-2xs text-muted-foreground">
                {hasOpenspec
                  ? 'Nothing being worked on right now.'
                  : 'This repository has no specs yet. Specs let you write down a plan before the code.'}
              </p>
              <button
                type="button"
                onClick={onNewChange}
                className="mt-1 flex items-center gap-1 text-2xs text-muted-foreground hover:text-accent-text"
              >
                <Plus size={11} strokeWidth={2} className="flex-none" />
                {hasOpenspec ? 'Start a new change' : 'Write your first spec'}
              </button>
            </div>
          ) : (
            changes.map((change) => (
              <SpecContextMenu key={change.id} change={change} repoId={repoId}>
                <SpecRow
                  change={change}
                  isSelected={change.id === selectedId}
                  onClick={() =>
                    // Clicking the selected row clears the pick rather than
                    // no-op'ing, so any row can be clicked away from.
                    selectChangeEverywhere(change.id === selectedId ? null : change.id)
                  }
                />
              </SpecContextMenu>
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
