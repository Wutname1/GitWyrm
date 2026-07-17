import type { CSSProperties } from 'react'
import { cn } from '@/lib/utils'
import type { SectionItem, SidebarSectionData } from '@/lib/types'
import { PendingIndicator } from '@/components/ui/pending-indicator'

function markerStyle(section: SidebarSectionData, item: SectionItem, isCurrent: boolean): CSSProperties {
  switch (section.type) {
    case 'tag':
      return { background: 'var(--gw-amber)', transform: 'rotate(45deg)', borderRadius: 1 }
    case 'pr':
      return {
        borderRadius: '50%',
        border: `2px solid ${
          item.state === 'merged' ? 'var(--gw-accent)' : item.state === 'draft' ? 'var(--gw-muted)' : 'var(--gw-green)'
        }`,
      }
    case 'issue':
      return { borderRadius: 2, background: 'var(--gw-red)' }
    case 'remote':
      return { borderRadius: '50%', border: '1.5px solid var(--gw-sub)' }
    case 'branch':
      return isCurrent
        ? { borderRadius: '50%', background: 'var(--gw-accent)' }
        : { borderRadius: '50%', border: '1.5px solid var(--gw-muted)' }
    default:
      return { borderRadius: 2, border: '1.5px solid var(--gw-muted)' }
  }
}

interface SectionItemRowProps {
  section: SidebarSectionData
  item: SectionItem
  isCurrent: boolean
  onClick: () => void
  onContextMenu?: (e: React.MouseEvent) => void
  disabled?: boolean
  pending?: boolean
  pendingLabel?: string
}

export function SectionItemRow({
  section,
  item,
  isCurrent,
  onClick,
  onContextMenu,
  disabled,
  pending,
  pendingLabel,
}: SectionItemRowProps) {
  return (
    <div
      onClick={disabled ? undefined : onClick}
      onContextMenu={onContextMenu}
      aria-disabled={disabled || undefined}
      aria-busy={pending || undefined}
      className={cn(
        'flex cursor-pointer items-center gap-2 py-1 pl-6 pr-3 transition-colors hover:bg-panel2',
        isCurrent && 'bg-soft',
        disabled && !pending && 'cursor-wait opacity-40',
        pending && 'cursor-wait bg-soft text-primary'
      )}
    >
      {pending ? (
        <PendingIndicator className="size-3 text-primary" />
      ) : (
        <span className="size-2 flex-none" style={markerStyle(section, item, isCurrent)} />
      )}
      <span
        className={cn(
          'overflow-hidden text-ellipsis whitespace-nowrap text-[11.5px]',
          isCurrent ? 'font-semibold text-foreground' : 'text-sub',
          pending && 'font-semibold text-primary'
        )}
      >
        {pending ? pendingLabel ?? 'Working…' : item.name}
      </span>
      {item.meta && (
        <span className="ml-auto whitespace-nowrap pl-1.5 font-mono text-[9.5px] text-muted-foreground">
          {item.meta}
        </span>
      )}
    </div>
  )
}
