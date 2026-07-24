import { forwardRef, type ComponentPropsWithoutRef, type CSSProperties, type ReactNode } from 'react'
import { Tag } from 'lucide-react'
import { cn } from '@/lib/utils'
import { GithubItemIcon } from '@/lib/githubDisplay'
import { laneColor } from '@/lib/gitDisplay'
import { useUiStore } from '@/stores/uiStore'
import { StashGlyph } from '@/components/domain/graph/StashGlyph'
import type { SectionItem, SidebarSectionData } from '@/lib/types'
import { PendingIndicator } from '@/components/ui/pending-indicator'
import { TooltipButton } from '@/components/ui/tooltip'

function markerStyle(section: SidebarSectionData, isCurrent: boolean): CSSProperties {
  switch (section.type) {
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

/**
 * The leading marker for a row. Tags, pull requests, issues and stashes each
 * get their own icon; everything else keeps the small colored dot.
 */
function RowMarker({
  section,
  item,
  isCurrent,
}: {
  section: SidebarSectionData
  item: SectionItem
  isCurrent: boolean
}) {
  // Match the color the graph drew this stash in. Until the graph has laid the
  // stash out (another view is open, or its page has not loaded) there is no
  // track to match, so fall back to the neutral lane color rather than
  // inventing one that would change under the user a moment later.
  const track = useUiStore((s) => (item.sha ? s.stashTracks[item.sha] : undefined))

  if (section.type === 'tag') {
    return <Tag aria-hidden className="size-2.5 flex-none text-[var(--gw-blue)]" />
  }
  if (section.type === 'pr' || section.type === 'issue') {
    return (
      <span className="flex-none">
        <GithubItemIcon kind={section.type === 'pr' ? 'pr' : 'issue'} size={11} />
      </span>
    )
  }
  if (section.type === 'stash') {
    return (
      <span className="flex-none">
        <StashGlyph size={12} color={laneColor(track ?? 0)} />
      </span>
    )
  }
  return <span className="size-2 flex-none" style={markerStyle(section, isCurrent)} />
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
        pending && 'cursor-wait bg-soft text-accent-text',
        className
      )}
    >
      {pending ? (
        <PendingIndicator className="size-3 text-accent-text" />
      ) : (
        <RowMarker section={section} item={item} isCurrent={isCurrent} />
      )}
      <span
        className={cn(
          'overflow-hidden text-ellipsis whitespace-nowrap text-xs',
          isCurrent ? 'font-semibold text-foreground' : 'text-sub',
          pending && 'font-semibold text-accent-text'
        )}
      >
        {pending ? pendingLabel ?? 'Working…' : item.name}
      </span>
      {item.meta && (
        <span
          title={item.metaTitle}
          className={cn(
            'ml-auto whitespace-nowrap pl-1.5 font-mono text-2xs',
            // Word markers ("new", "gone") are states, not counts -- give them
            // their own weight so they don't read as an ahead/behind number.
            // Sync counts and stash ages are quiet data, not calls to action.
            /^[↑↓]/.test(item.meta) || section.type === 'stash'
              ? 'text-muted-foreground'
              : 'font-semibold uppercase tracking-wide text-accent-text/80'
          )}
        >
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
