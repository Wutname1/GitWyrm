import { Plus } from 'lucide-react'
import { cn } from '@/lib/utils'

/**
 * The sidebar's own "New chat" affordance (mockup `.ag-new-chat`), separate
 * from `AgentDeskTitleBar`'s title-bar button -- the mockup keeps both: the
 * title bar's is always reachable, this one lives where the eye already is
 * when scanning the session list. Both call the same injected callback per
 * the seam contract; which one exists in a given layout is a shell decision,
 * not this component's.
 */
export function NewSessionButton({
  onNewSession,
  disabled,
}: {
  onNewSession: () => void
  disabled?: boolean
}) {
  return (
    <button
      type="button"
      onClick={onNewSession}
      disabled={disabled}
      className={cn(
        'flex h-[31px] w-full items-center gap-2 rounded-md border border-border bg-panel2 px-2.5',
        'text-xs font-semibold text-foreground',
        'hover:bg-panel3 disabled:cursor-not-allowed disabled:opacity-50'
      )}
    >
      <Plus size={14} strokeWidth={2.25} />
      New chat
    </button>
  )
}
