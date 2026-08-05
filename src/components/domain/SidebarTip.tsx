import { useWorkspaceStore } from '@/stores/workspaceStore'
import { cn } from '@/lib/utils'

/**
 * A short explanation that teaches a feature rather than labelling a control.
 *
 * Gated on one setting so "stop explaining things to me" is a single switch
 * instead of a dismiss button per tip. Rendering nothing is always safe here:
 * a tip never carries state, a count, or the only way to reach an action, so
 * hiding one can never leave a section looking broken.
 *
 * This is for prose only. Errors, warnings, and empty-state labels are not
 * tips -- they report what is true right now, and must survive the switch.
 */
export function SidebarTip({
  children,
  className,
  indent = true,
}: {
  children: React.ReactNode
  className?: string
  /** Line up with sidebar rows. Off for tips laid out by their own container. */
  indent?: boolean
}) {
  const showTips = useWorkspaceStore((s) => s.showTips)
  if (!showTips) return null

  return (
    <div
      style={indent ? { paddingLeft: 24 } : undefined}
      className={cn('pr-3 pt-1 text-2xs leading-relaxed text-muted-foreground', className)}
    >
      {children}
    </div>
  )
}
