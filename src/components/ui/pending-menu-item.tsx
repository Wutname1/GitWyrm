import type { ComponentProps, ReactNode } from 'react'
import { ContextMenuItem } from '@/components/ui/context-menu'
import { PendingIndicator } from '@/components/ui/pending-indicator'

interface PendingMenuItemProps {
  icon: ReactNode
  label: string
  /** Shown, with a spinner in place of the icon, while `pending`. */
  pendingLabel: string
  pending?: boolean
  disabled?: boolean
  variant?: ComponentProps<typeof ContextMenuItem>['variant']
  onRun: () => void
}

/**
 * A context-menu item that runs an action and shows its progress in place.
 *
 * Radix closes the menu after `onSelect` unless the event is defaulted, so the
 * `preventDefault` that keeps a running action visible lives here rather than
 * being repeated (and occasionally forgotten) at each site.
 */
export function PendingMenuItem({
  icon,
  label,
  pendingLabel,
  pending = false,
  disabled,
  variant,
  onRun,
}: PendingMenuItemProps) {
  return (
    <ContextMenuItem
      variant={variant}
      disabled={disabled}
      onSelect={(e) => {
        e.preventDefault()
        onRun()
      }}
    >
      {pending ? <PendingIndicator /> : icon}
      {pending ? pendingLabel : label}
    </ContextMenuItem>
  )
}
