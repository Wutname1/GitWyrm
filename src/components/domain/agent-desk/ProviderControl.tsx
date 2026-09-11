import { useQuery } from '@tanstack/react-query'
import { Bot, Check, ChevronUp } from 'lucide-react'
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
  trigger,
  side = 'top',
}: {
  sessionId: string | null
  provider: string | null
  onChange: (provider: string | null) => void
  open: boolean
  onOpenChange: (open: boolean) => void
  /**
   * The element the list hangs off, when it is not the compact pill.
   *
   * The new-chat screen has its own full-width "Which AI?" row in the middle
   * of the pane. It used to open this list by reaching over and toggling the
   * COMPOSER's copy, which is the only other place this control is mounted --
   * so the list appeared anchored to the bottom bar, a long way from the
   * button that had just been clicked, overlapping the composer. A popover
   * has to hang off the thing you pressed.
   */
  trigger?: React.ReactNode
  side?: 'top' | 'bottom'
}) {
  const query = useQuery({
    queryKey: keys.agentProviders(sessionId),
    queryFn: async () => unwrap(await commands.agentProvidersList(sessionId)),
    // Only ask while the list is on screen.
    enabled: open,
    staleTime: 0,
  })

  const allRows = query.data?.providers ?? []
  // Whether this chat may change files is the backend's answer, not one
  // re-derived here. An earlier version worked it out from the composer's
  // mode pill and got a different answer than the engine's own tool gate, so
  // the picker offered a tool for a Review chat that the launch then refused.
  const readOnly = query.data?.readOnly ?? false
  /**
   * Only tools that can actually run this chat.
   *
   * This list used to show every tool GitWyrm knows about, disabled ones
   * included, on the argument that "a tool missing from the list looks like a
   * bug; a tool listed with 'not installed' beside it is an answer." That
   * holds for a list of three or four. It does not hold here: most people
   * have one or two of these installed, so the list was mostly rows that
   * could not be picked, each carrying a sentence explaining why -- and the
   * two or three real choices were the minority of what was on screen.
   *
   * A tool the chat cannot use is still named, once, in the line under the
   * list, so nothing disappears without explanation.
   */
  const rows = usableProviders(allRows, readOnly)
  const hiddenCount = allRows.length - rows.length
  const chosen = allRows.find((r) => r.id === provider)
  const label = chosen?.displayName ?? (provider ?? 'Default AI')

  return (
    <Popover open={open} onOpenChange={onOpenChange}>
      <PopoverTrigger asChild>
        {trigger ?? (
          <button
            type="button"
            className="flex flex-none items-center gap-1 rounded px-1.5 py-0.5 text-2xs font-semibold text-sub hover:bg-panel3 hover:text-foreground"
          >
            <Bot size={12} />
            {label}
            <ChevronUp size={11} />
          </button>
        )}
      </PopoverTrigger>
      <PopoverContent side={side} align="start" className="w-80 p-2">
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
                disabled={query.isFetching}
              className="mt-1.5 rounded border border-border px-1.5 py-0.5 text-2xs font-semibold hover:bg-panel3"
            >
              {query.isFetching ? 'Checking…' : 'Try again'}
            </button>
          </div>
        ) : rows.length === 0 ? (
          <p className="px-1 py-2 text-2xs leading-relaxed text-muted-foreground">
            This build of GitWyrm has no AI tools set up.
          </p>
        ) : (
          <div className="flex flex-col gap-0.5" role="listbox" aria-label="AI tool">
            <ProviderRow
              label="Default AI"
              selected={provider === null}
              onSelect={() => {
                onChange(null)
                onOpenChange(false)
              }}
            />
            {rows.map((row) => (
              <ProviderRow
                key={row.id}
                label={row.displayName}
                note={row.isDefault ? 'default' : undefined}
                selected={provider === row.id}
                onSelect={() => {
                  onChange(row.id)
                  onOpenChange(false)
                }}
              />
            ))}
            {/* Named, not silently dropped. One quiet line is the difference
                between a short list and a list that looks like it is missing
                something. */}
            {hiddenCount > 0 && (
              <p className="px-2 pt-1 text-2xs text-muted-foreground">
                {hiddenCount === 1
                  ? '1 other AI tool cannot be used for this chat.'
                  : `${hiddenCount} other AI tools cannot be used for this chat.`}
              </p>
            )}
          </div>
        )}
      </PopoverContent>
    </Popover>
  )
}

/**
 * The tools this chat can actually be run with.
 *
 * The list used to include every tool GitWyrm knows about, disabled, each
 * with a sentence explaining why it could not be used. For someone with one
 * or two of these installed that is a list where the real choices are
 * outnumbered by the ones that are not choices at all.
 *
 * Exported so the rule is testable on its own: which tools appear is the
 * whole behaviour of this control, and it is decided here rather than in the
 * markup.
 */
export function usableProviders(rows: AgentProvider[], readOnly: boolean): AgentProvider[] {
  return rows.filter((row) => blockedReason(row, readOnly) === undefined)
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
  // Checked before `installed`, because an unresponsive tool reports
  // `installed: false` and would otherwise be told to install something it can
  // see on disk.
  if (row.unresponsive) {
    return 'Found on this computer, but it did not answer when GitWyrm asked its version. It may be busy, or need reinstalling.'
  }
  if (!row.installed) {
    return 'Not installed on this computer.'
  }
  if (readOnly && !row.canDoReadOnlyWork) {
    return row.readOnlyLimit ?? 'Cannot be used for a chat that must not change anything.'
  }
  return undefined
}


/**
 * One pickable tool: its name, and a marker if it is the default.
 *
 * No description line. Every row used to carry one ("Can be used for any kind
 * of chat"), which said the same thing about nearly every row and tripled the
 * height of a list whose whole job is to let someone pick a name they already
 * know. Rows that could NOT be picked carried their reason here, which was
 * worth reading -- and those rows are no longer in the list at all.
 */
function ProviderRow({
  label,
  note,
  selected,
  onSelect,
}: {
  label: string
  note?: string
  selected: boolean
  onSelect: () => void
}) {
  return (
    <button
      type="button"
      role="option"
      aria-selected={selected}
      onClick={onSelect}
      className={cn(
        'flex items-center gap-2 rounded-md px-2 py-1.5 text-left hover:bg-panel3',
        selected && 'bg-soft'
      )}
    >
      <Bot size={13} className="flex-none text-muted-foreground" aria-hidden />
      <strong className="min-w-0 flex-1 truncate text-2xs font-semibold text-foreground">{label}</strong>
      {note && <span className="flex-none font-mono text-2xs text-muted-foreground">{note}</span>}
      {selected && <Check size={12} className="flex-none text-accent-text" aria-hidden />}
    </button>
  )
}
