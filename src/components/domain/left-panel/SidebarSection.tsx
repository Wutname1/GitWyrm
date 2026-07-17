import type { ReactNode } from 'react'
import { ChevronRight, Plus } from 'lucide-react'
import { cn } from '@/lib/utils'
import type { SidebarSectionData, SectionItem } from '@/lib/types'
import { useUiStore } from '@/stores/uiStore'
import { SectionItemRow } from './SectionItemRow'

interface SidebarSectionProps {
  section: SidebarSectionData
  currentBranch: string
  onItemClick: (section: SidebarSectionData, item: SectionItem) => void
  onItemContextMenu?: (section: SidebarSectionData, item: SectionItem, e: React.MouseEvent) => void
  /** Wraps a row in a right-click menu. Return null for items with no actions. */
  renderItemMenu?: (
    section: SidebarSectionData,
    item: SectionItem,
    row: ReactNode
  ) => ReactNode
  /** When set, a `+` button appears on hover in the section header. */
  onAdd?: () => void
  addLabel?: string
}

export function SidebarSection({
  section,
  currentBranch,
  onItemClick,
  onItemContextMenu,
  renderItemMenu,
  onAdd,
  addLabel,
}: SidebarSectionProps) {
  const open = useUiStore((s) => s.sectionOpen[section.key])
  const toggleSection = useUiStore((s) => s.toggleSection)

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
        <span className="text-[10px] font-bold tracking-[.09em] text-sub">{section.label}</span>
        {onAdd && (
          <button
            onClick={(e) => {
              e.stopPropagation()
              onAdd()
            }}
            title={addLabel ?? 'Add'}
            className="ml-auto flex size-4 flex-none items-center justify-center rounded text-muted-foreground opacity-0 hover:bg-panel3 hover:text-foreground focus:opacity-100 group-hover/section:opacity-100"
          >
            <Plus size={12} strokeWidth={2.4} />
          </button>
        )}
        <span
          className={cn(
            'font-mono text-[9.5px] text-muted-foreground',
            onAdd ? 'ml-1.5' : 'ml-auto'
          )}
        >
          {section.items.length}
        </span>
      </div>
      {open && (
        <div className="pb-1">
          {section.items.map((item) => {
            const row = (
              <SectionItemRow
                section={section}
                item={item}
                isCurrent={section.type === 'branch' && item.name === currentBranch}
                onClick={() => onItemClick(section, item)}
                onContextMenu={
                  onItemContextMenu ? (e) => onItemContextMenu(section, item, e) : undefined
                }
              />
            )
            const wrapped = renderItemMenu?.(section, item, row)
            return <div key={item.name}>{wrapped ?? row}</div>
          })}
        </div>
      )}
    </div>
  )
}
