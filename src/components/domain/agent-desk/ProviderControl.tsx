import { useQuery } from '@tanstack/react-query'
import { Bot, ChevronUp, CircleAlert, Download } from 'lucide-react'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { commands } from '@/lib/bindings'
import type { AgentProvider } from '@/lib/bindings'
import { keys, unwrap } from '@/lib/queryKeys'
import { cn } from '@/lib/utils'

/**
 * "Which AI does this chat use?" popover.
 *
 * Sits beside the mode and team controls and is independent of both: those
 * decide what an agent may do and how many there are, this decides which
 * tool. `null` means "whatever the default is", which is what a chat uses
 * until someone picks otherwise -- deliberately not resolved to the default
 * tool's id here, so a chat that was never given a preference keeps following
 * the default if it ever changes.
 *
 * Options that cannot run the work are shown, disabled, with the reason
 * attached rather than hidden. A tool missing from the list looks like a bug;
 * a tool listed with "not installed" beside it is an answer. The same applies
 * to a tool that cannot be trusted with read-only work -- see
 * `read_only_limit` in `commands::agent_providers`.
 *
 * The list is re-fetched whenever the popover opens rather than cached for
 * the session, because installing one of these tools is exactly the thing a
 * user does right after opening this list and finding it missing. The backend
 * probe caches a found tool but never a missing one, so this stays cheap.
 */
export function ProviderControl({
  sessionId,
  provider,
  onChange,
  open,
  onOpenChange,
}: {
  sessionId: string | null
  provider: string | null
  onChange: (provider: string | null) => void
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  const query = useQuery({
    queryKey: keys.agentProviders(sessionId),
    queryFn: async () => unwrap(await commands.agentProvidersList(sessionId)),
    // Only ask while the list is on screen.
    enabled: open,
    staleTime: 0,
  })

  const rows = query.data?.providers ?? []
  // Whether this chat may change files is the backend's answer, not one
  // re-derived here. An earlier version worked it out from the composer's
  // mode pill and got a different answer than the engine's own tool gate, so
  // the picker offered a tool for a Review chat that the launch then refused.
  const readOnly = query.data?.readOnly ?? false
  const chosen = rows.find((r) => r.id === provider)
  const label = chosen?.displayName ?? (provider ?? 'Default AI')

  return (
    <Popover open={open} onOpenChange={onOpenChange}>
      <PopoverTrigger asChild>
        <button
          type="button"
          className="flex flex-none items-center gap-1 rounded px-1.5 py-0.5 text-2xs font-semibold text-sub hover:bg-panel3 hover:text-foreground"
        >
          <Bot size={12} />
          {label}
          <ChevronUp size={11} />
        </button>
      </PopoverTrigger>
      <PopoverContent side="top" align="start" className="w-80 p-2">
        <div className="mb-2 flex items-center gap-2">
          <Bot size={13} className="text-accent-text" />
          <strong className="text-xs font-semibold">Which AI does this chat use?</strong>
        </div>

        {query.isLoading ? (
          <p className="px-1 py-2 text-2xs text-muted-foreground">Looking for installed AI tools…</p>
        ) : query.isError ? (
          // A failed check and an empty list are different problems and need
          // different words. This one has a button that actually retries,
          // rather than copy telling the user to try again with nothing to
          // press.
          <div className="px-1 py-2">
            <p className="text-2xs leading-relaxed text-muted-foreground">
              GitWyrm could not check which AI tools you have.
            </p>
            <button
              type="button"
              onClick={() => void query.refetch()}
              className="mt-1.5 rounded border border-border px-1.5 py-0.5 text-2xs font-semibold hover:bg-panel3"
            >
              Try again
            </button>
          </div>
        ) : rows.length === 0 ? (
          <p className="px-1 py-2 text-2xs leading-relaxed text-muted-foreground">
            This build of GitWyrm has no AI tools set up.
          </p>
        ) : (
          <div className="flex flex-col gap-1.5" role="listbox" aria-label="AI tool">
            <ProviderRow
              label="Default AI"
              detail="Use whichever tool GitWyrm is set up to use."
              selected={provider === null}
              onSelect={() => {
                onChange(null)
                onOpenChange(false)
              }}
            />
            {rows.map((row) => {
              const blocked = blockedReason(row, readOnly)
              return (
                <ProviderRow
                  key={row.id}
                  label={row.displayName}
                  detail={detailFor(row, readOnly)}
                  note={row.isDefault ? 'default' : row.version ?? undefined}
                  selected={provider === row.id}
                  blocked={blocked}
                  onSelect={() => {
                    onChange(row.id)
                    onOpenChange(false)
                  }}
                />
              )
            })}
          </div>
        )}
      </PopoverContent>
    </Popover>
  )
}

/**
 * Why this tool cannot be picked for this chat, or `undefined` when it can.
 *
 * Two separate reasons, and they need different words: one is fixed by
 * installing something, the other cannot be fixed at all for this kind of
 * chat and means picking a different tool.
 */
export function blockedReason(row: AgentProvider, readOnly: boolean): string | undefined {
  if (row.tooOld) {
    return `Found ${row.version ?? 'an older version'}, which is too old for GitWyrm to use. Updating it fixes this.`
  }
  if (!row.installed) {
    return 'Not installed on this computer.'
  }
  if (readOnly && !row.canDoReadOnlyWork) {
    return row.readOnlyLimit ?? 'Cannot be used for a chat that must not change anything.'
  }
  return undefined
}

export function detailFor(row: AgentProvider, readOnly: boolean): string {
  if (row.canDoReadOnlyWork) return 'Can be used for any kind of chat.'
  // Only worth saying where it bites. On a chat that is allowed to change
  // files this tool is a perfectly ordinary choice, and printing its
  // limitation under an enabled row reads as a warning about picking it.
  if (readOnly) return 'Can only be used for chats that are allowed to change files.'
  return 'Can be used for this chat.'
}

function ProviderRow({
  label,
  detail,
  note,
  selected,
  blocked,
  onSelect,
}: {
  label: string
  detail: string
  note?: string
  selected: boolean
  blocked?: string
  onSelect: () => void
}) {
  const disabled = blocked !== undefined
  return (
    <button
      type="button"
      // `aria-disabled` rather than `disabled`: a real `disabled` button is
      // removed from the tab order, so a keyboard or screen-reader user never
      // reaches the row and never hears why it cannot be used -- which makes
      // the explanation decorative for exactly the people who most need it
      // read aloud. Focusable and announced, with the click guarded instead.
      aria-disabled={disabled}
      role="option"
      aria-selected={selected && !disabled}
      onClick={() => {
        if (!disabled) onSelect()
      }}
      // A disabled row still has to be readable: the reason it is disabled is
      // the most useful thing on it, so the text stays legible and only the
      // affordance is dimmed.
      className={cn(
        'flex items-start gap-2 rounded-md border border-border px-2 py-1.5 text-left',
        selected && !disabled && 'border-primary/50 bg-soft',
        disabled ? 'cursor-not-allowed opacity-70' : 'hover:bg-panel3'
      )}
    >
      {disabled ? (
        <CircleAlert size={14} className="mt-0.5 flex-none text-muted-foreground" aria-hidden />
      ) : (
        <Bot size={14} className="mt-0.5 flex-none text-muted-foreground" aria-hidden />
      )}
      <span className="min-w-0 flex-1">
        <strong className="block text-2xs font-semibold text-foreground">{label}</strong>
        <span className="block text-2xs leading-snug text-muted-foreground">
          {blocked ?? detail}
        </span>
      </span>
      {note && !disabled && (
        <span className="flex-none font-mono text-[9px] text-muted-foreground">{note}</span>
      )}
      {disabled && blocked?.startsWith('Not installed') && (
        <Download size={12} className="mt-0.5 flex-none text-muted-foreground" aria-hidden />
      )}
    </button>
  )
}
