import specDeskIcon from '@/assets/icons/specdesk.png'
import { cn } from '@/lib/utils'
import { DisabledHint, TooltipButton } from '@/components/ui/tooltip'
import { openSpecDesk } from '@/lib/specDesk'
import { useOpenspecStatus } from '@/hooks/useOpenspec'
import { useActiveRepo } from '@/stores/workspaceStore'

/**
 * Toolbar action for opening the Spec Desk window.
 *
 * The Desk already opens from the sidebar, spec cards, graph chips, and context
 * menus, but all of those need a change to exist first. This is the one place
 * that is always in the same spot, so the Desk is reachable even from a repo
 * whose specs are not on screen.
 *
 * A repo without an `openspec/` folder has no Desk to show. The button stays in
 * place and says so on hover rather than disappearing, so the toolbar does not
 * reshuffle as you move between repositories.
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
  const status = useOpenspecStatus(repo?.id ?? null)

  // Until the check comes back, treat the repo as having specs. Guessing the
  // other way would flash the button off and then on for every OpenSpec repo.
  const hasSpecs = status.data?.present ?? true
  const off = disabled || !hasSpecs
  const reason = disabled
    ? disabledReason
    : "This repository doesn't use specs yet"

  return (
    <DisabledHint disabled={off} reason={reason}>
      <TooltipButton
        onClick={() => repo && void openSpecDesk(repo.id)}
        tooltip={off && reason ? reason : 'Open Spec Desk'}
        aria-label="Open Spec Desk"
        disabled={off}
        className={cn(
          'group flex h-[30px] w-8 items-center justify-center rounded-md border border-border bg-panel2 text-sub hover:border-muted-foreground hover:bg-panel3 disabled:pointer-events-none',
          off && 'opacity-35'
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
