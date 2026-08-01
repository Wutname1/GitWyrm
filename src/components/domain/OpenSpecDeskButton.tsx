import specDeskIcon from '@/assets/icons/specdesk.png'
import { cn } from '@/lib/utils'
import { DisabledHint, TooltipButton } from '@/components/ui/tooltip'
import { openSpecDesk } from '@/lib/specDesk'
import { useActiveRepo, useWorkspaceStore } from '@/stores/workspaceStore'

/**
 * Toolbar action for opening the Spec Desk window.
 *
 * The Desk already opens from the sidebar, spec cards, graph chips, and context
 * menus, but all of those need a change to exist first. This is the one place
 * that is always in the same spot, so the Desk is reachable even from a repo
 * whose specs are not on screen.
 *
 * Deliberately not gated on the repo already having an `openspec/` folder: a
 * repo that has never used specs is the one that most needs the way in. Users
 * who don't plan this way turn the whole feature off in Settings > OpenSpec,
 * which takes the button away entirely.
 */
export function OpenSpecDeskButton({
  disabled,
  disabledReason,
}: {
  disabled?: boolean
  /** Why the button is off, shown on hover in place of the usual tooltip. */
  disabledReason?: string
}) {
  const repo = useActiveRepo()
  const specDeskEnabled = useWorkspaceStore((s) => s.enableSpecDesk)

  if (!specDeskEnabled) return null

  return (
    <DisabledHint disabled={!!disabled} reason={disabledReason}>
      <TooltipButton
        onClick={() => repo && void openSpecDesk(repo.id)}
        tooltip={disabled && disabledReason ? disabledReason : 'Open Spec Desk'}
        aria-label="Open Spec Desk"
        disabled={disabled}
        className={cn(
          'group flex h-[30px] w-8 items-center justify-center rounded-md border border-border bg-panel2 text-sub hover:border-muted-foreground hover:bg-panel3 disabled:pointer-events-none',
          disabled && 'opacity-35'
        )}
      >
        {/* The mark carries its own accent color, so it is rendered as an
            image rather than tinted from the button's text color. */}
        <img
          src={specDeskIcon}
          alt=""
          aria-hidden
          width={16}
          height={16}
          style={{ width: 16, height: 16 }}
        />
      </TooltipButton>
    </DisabledHint>
  )
}
