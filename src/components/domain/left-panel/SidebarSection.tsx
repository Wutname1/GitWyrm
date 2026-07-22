import type { ReactNode } from 'react'
import { ChevronRight, Plus } from 'lucide-react'
import { cn } from '@/lib/utils'
import type { SidebarSectionData, SectionItem } from '@/lib/types'
import { useUiStore } from '@/stores/uiStore'
import { TooltipButton } from '@/components/ui/tooltip'
import { SectionItemRow } from './SectionItemRow'

interface SidebarSectionProps {
  section: SidebarSectionData
  currentBranch: string
  onItemClick: (section: SidebarSectionData, item: SectionItem) => void
  onItemDoubleClick?: (section: SidebarSectionData, item: SectionItem) => void
  onItemContextMenu?: (section: SidebarSectionData, item: SectionItem, e: React.MouseEvent) => void
  /** Wraps a row in a right-click menu. Return null for items with no actions. */
  renderItemMenu?: (
    section: SidebarSectionData,
    item: SectionItem,
    row: ReactNode
  ) => ReactNode
  /**
   * Fully render an item instead of the default row (used to give branch rows
   * their own drag-and-drop wiring). Gets a `renderMenu` that wraps a row in the
   * section's right-click menu. When omitted, the default row + `renderItemMenu`
   * path is used.
   */
  renderItem?: (
    section: SidebarSectionData,
    item: SectionItem,
    ctx: {
      isCurrent: boolean
      renderMenu: (row: ReactNode) => ReactNode
    }
  ) => ReactNode
  /** When set, a `+` button appears on hover in the section header. */
  onAdd?: () => void
  addLabel?: string
  isItemPending?: (section: SidebarSectionData, item: SectionItem) => boolean
  isItemDisabled?: (section: SidebarSectionData, item: SectionItem) => boolean
  getPendingLabel?: (section: SidebarSectionData, item: SectionItem) => string
  getHoverAction?: (
    section: SidebarSectionData,
    item: SectionItem
  ) => { icon: ReactNode; title: string; onClick: () => void } | undefined
}

export function SidebarSection({
  section,
  currentBranch,
  onItemClick,
  onItemDoubleClick,
  onItemContextMenu,
  renderItemMenu,
  renderItem,
  onAdd,
  addLabel,
  isItemPending,
  isItemDisabled,
  getPendingLabel,
  getHoverAction,
}: SidebarSectionProps) {
  const open = useUiStore((s) => s.sectionOpen[section.key])
  const toggleSection = useUiStore((s) => s.toggleSection)
  const githubItem = useUiStore((s) => s.githubItem)
  const centerView = useUiStore((s) => s.centerView)

  // A branch row is "current" when it is checked out; a PR/issue row is
  // "current" when that item is the one open in the center view.
  const isCurrentItem = (item: SectionItem) => {
    if (section.type === 'branch') return item.name === currentBranch
    if (section.type === 'pr' || section.type === 'issue') {
      return (
        centerView === 'github' &&
        githubItem?.kind === (section.type === 'pr' ? 'pr' : 'issue') &&
        githubItem.number === item.id
      )
    }
    return false
  }

  return (
    <div className="group/section">
      <div
        onClick={() => toggleSection(section.key)}
        className="flex cursor-pointer select-none items-center gap-1.5 py-1.5 pl-2.5 pr-3 hover:bg-panel2"
      >
        <ChevronRight
          size={12}
          strokeWidth={2.4}
          className={cn('flex-none text-muted-foreground transition-transform duration-100', open && 'rotate-90')}
        />
        <span className="text-2xs font-bold tracking-[.09em] text-sub">{section.label}</span>
        {onAdd && (
          <TooltipButton
            onClick={(e) => {
              e.stopPropagation()
              onAdd()
            }}
            tooltip={addLabel ?? 'Add'}
            className="ml-auto flex size-4 flex-none items-center justify-center rounded text-muted-foreground opacity-0 hover:bg-panel3 hover:text-foreground focus:opacity-100 group-hover/section:opacity-100"
          >
            <Plus size={12} strokeWidth={2.4} />
          </TooltipButton>
        )}
        <span
          className={cn(
            'font-mono text-2xs text-muted-foreground',
            onAdd ? 'ml-1.5' : 'ml-auto'
          )}
        >
          {section.items.length}
        </span>
      </div>
      {open && (
        <div className="pb-1">
          {section.items.map((item) => {
            const key = item.sha ?? item.id ?? item.name
            if (renderItem) {
              const custom = renderItem(section, item, {
                isCurrent: isCurrentItem(item),
                renderMenu: (row) => renderItemMenu?.(section, item, row) ?? row,
              })
              if (custom !== undefined) return <div key={key}>{custom}</div>
            }
            const row = (
              <SectionItemRow
                section={section}
                item={item}
                isCurrent={isCurrentItem(item)}
                onClick={() => onItemClick(section, item)}
                onDoubleClick={
                  onItemDoubleClick ? () => onItemDoubleClick(section, item) : undefined
                }
                hoverAction={getHoverAction?.(section, item)}
                pending={isItemPending?.(section, item)}
                disabled={isItemDisabled?.(section, item)}
                pendingLabel={getPendingLabel?.(section, item)}
                onContextMenu={
                  onItemContextMenu ? (e) => onItemContextMenu(section, item, e) : undefined
                }
              />
            )
            const wrapped = renderItemMenu?.(section, item, row)
            return <div key={key}>{wrapped ?? row}</div>
          })}
        </div>
      )}
    </div>
  )
}
