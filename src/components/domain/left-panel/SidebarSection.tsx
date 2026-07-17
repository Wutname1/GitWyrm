import { ChevronRight } from 'lucide-react'
import { cn } from '@/lib/utils'
import type { SidebarSectionData, SectionItem } from '@/lib/types'
import { useUiStore } from '@/stores/uiStore'
import { SectionItemRow } from './SectionItemRow'

interface SidebarSectionProps {
  section: SidebarSectionData
  currentBranch: string
  onItemClick: (section: SidebarSectionData, item: SectionItem) => void
  onItemContextMenu?: (section: SidebarSectionData, item: SectionItem, e: React.MouseEvent) => void
}

export function SidebarSection({
  section,
  currentBranch,
  onItemClick,
  onItemContextMenu,
}: SidebarSectionProps) {
  const open = useUiStore((s) => s.sectionOpen[section.key])
  const toggleSection = useUiStore((s) => s.toggleSection)

  return (
    <div>
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
        <span className="ml-auto font-mono text-[9.5px] text-muted-foreground">{section.items.length}</span>
      </div>
      {open && (
        <div className="pb-1">
          {section.items.map((item) => (
            <SectionItemRow
              key={item.name}
              section={section}
              item={item}
              isCurrent={section.type === 'branch' && item.name === currentBranch}
              onClick={() => onItemClick(section, item)}
              onContextMenu={
                onItemContextMenu ? (e) => onItemContextMenu(section, item, e) : undefined
              }
            />
          ))}
        </div>
      )}
    </div>
  )
}
