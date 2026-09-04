import specDeskIcon from '@/assets/icons/specdesk.png'
import { useAgentSessionHeaders } from '@/hooks/useAgentSessions'
import { summarizeAgentActivity } from '@/lib/agentSessionGrouping'
import { cn } from '@/lib/utils'
import { DisabledHint, TooltipButton } from '@/components/ui/tooltip'
import { openSpecDesk } from '@/lib/specDesk'
import { useActiveRepo, useWorkspaceStore } from '@/stores/workspaceStore'

/**
 * Toolbar action for opening the Agent Desk window.
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
 *
 * "Spec Desk" is renamed to "Agent Desk" in every user-facing string here,
 * but the persisted setting key (`enableSpecDesk`/`enable_spec_desk`) and the
 * `openSpecDesk` function name stay as-is -- see `src/lib/specDesk.ts`.
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
  // App-wide on purpose, not scoped to the open repo: the Desk is one window
  // for every project, so a run finishing in another project still has to be
  // able to reach the person. This button is the only thing permanently on
  // screen while they work, which makes it the one place a signal can land.
  const { headers } = useAgentSessionHeaders()
  const activity = summarizeAgentActivity(headers)

  if (!specDeskEnabled) return null

  return (
    <DisabledHint disabled={!!disabled} reason={disabledReason}>
      <TooltipButton
        onClick={() => repo && void openSpecDesk(repo.id)}
        tooltip={
          disabled && disabledReason ? disabledReason : activity.tone ? `Agent Desk -- ${activity.label}` : 'Open Agent Desk'
        }
        aria-label={activity.tone ? `Open Agent Desk, ${activity.label}` : 'Open Agent Desk'}
        disabled={disabled}
        className={cn(
          'group relative flex h-[30px] w-8 items-center justify-center rounded-md border border-border bg-panel2 text-sub hover:border-muted-foreground hover:bg-panel3 disabled:pointer-events-none',
          disabled && 'opacity-35'
        )}
      >
        {activity.tone && (
          // A dot, not a number: the count is in the tooltip and the
          // accessible name, and a numeral at this size would be unreadable
          // while adding nothing the person acts on differently.
          <span
            className={cn(
              'absolute right-0.5 top-0.5 block h-1.5 w-1.5 rounded-full',
              activity.tone === 'needsYou'
                ? 'bg-[var(--gw-amber)] shadow-[0_0_0_2px_var(--gw-panel2)]'
                : 'bg-[var(--gw-blue)] shadow-[0_0_0_2px_var(--gw-panel2)]'
            )}
            aria-hidden
          />
        )}
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
