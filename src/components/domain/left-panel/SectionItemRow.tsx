import { forwardRef, type ComponentPropsWithoutRef, type CSSProperties, type ReactNode } from 'react'
import { cn } from '@/lib/utils'
import type { SectionItem, SidebarSectionData } from '@/lib/types'
import { PendingIndicator } from '@/components/ui/pending-indicator'
import { TooltipButton } from '@/components/ui/tooltip'

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

interface SectionItemRowProps extends Omit<ComponentPropsWithoutRef<'div'>, 'onClick' | 'onDoubleClick'> {
  section: SidebarSectionData
  item: SectionItem
  isCurrent: boolean
  onClick: () => void
  onDoubleClick?: () => void
  disabled?: boolean
  pending?: boolean
  pendingLabel?: string
  /** Action shown on the right of the row on hover (e.g. a quick-switch icon). */
  hoverAction?: { icon: ReactNode; title: string; onClick: () => void }
}

/**
 * A row in the left panel. Forwards its ref and any extra props to the root
 * div so a Radix `asChild` trigger (the right-click menu) can attach its own
 * handlers -- without that, the menu silently never opens.
 */
export const SectionItemRow = forwardRef<HTMLDivElement, SectionItemRowProps>(function SectionItemRow(
  {
    section,
    item,
    isCurrent,
    onClick,
    onDoubleClick,
    disabled,
    pending,
    pendingLabel,
    hoverAction,
    className,
    ...rest
  },
  ref
) {
  return (
    <div
      ref={ref}
      {...rest}
      onClick={disabled ? undefined : onClick}
      onDoubleClick={disabled ? undefined : onDoubleClick}
      aria-disabled={disabled || undefined}
      aria-busy={pending || undefined}
      className={cn(
        'group/row flex cursor-pointer items-center gap-2 py-1 pl-6 pr-3 transition-colors hover:bg-panel2',
        isCurrent && 'bg-soft',
        disabled && !pending && 'cursor-wait opacity-40',
        pending && 'cursor-wait bg-soft text-primary',
        className
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
      {hoverAction && !pending && (
        <TooltipButton
          onClick={(e) => {
            e.stopPropagation()
            hoverAction.onClick()
          }}
          tooltip={hoverAction.title}
          className={cn(
            'flex size-4 flex-none items-center justify-center rounded text-muted-foreground opacity-0 hover:bg-panel3 hover:text-foreground focus:opacity-100 group-hover/row:opacity-100',
            item.meta ? 'ml-1.5' : 'ml-auto'
          )}
        >
          {hoverAction.icon}
        </TooltipButton>
      )}
    </div>
  )
})
