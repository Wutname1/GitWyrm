import { cn } from '@/lib/utils'
import { useAgentDeskPanelDrag } from '@/hooks/useAgentDeskPanelDrag'
import type { DockZone } from '@/lib/agentDeskDockPlacement'

/**
 * One invisible-until-dragging drop zone at an edge of the workspace
 * (tasks.md 7.5/7.6): highlighted only while a panel drag is over it,
 * accepting a drop by calling back with its own zone. Reused for right,
 * bottom, and both left placements -- callers position it via `className`.
 */
export function AgentDeskDockDropZone({
  zone,
  onDrop,
  className,
  label,
}: {
  zone: DockZone
  onDrop: (zone: DockZone) => void
  className?: string
  label: string
}) {
  const { dropTargetProps, dragging, hoverZone } = useAgentDeskPanelDrag('source', 'popover')
  const isHover = hoverZone === zone

  if (!dragging) return null

  return (
    <div
      {...dropTargetProps(zone, onDrop)}
      aria-label={`Drop to dock ${label}`}
      className={cn(
        'pointer-events-auto z-40 flex items-center justify-center rounded-md border-2 border-dashed border-border bg-panel/80 text-2xs font-semibold text-muted-foreground backdrop-blur-sm transition-colors',
        isHover && 'border-primary bg-soft text-foreground',
        className
      )}
    >
      {label}
    </div>
  )
}
