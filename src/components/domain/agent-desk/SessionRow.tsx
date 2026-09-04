import { useState } from 'react'
import {
  Archive,
  ArchiveRestore,
  Pencil,
  Trash2,
  Bot,
} from 'lucide-react'
import { toast } from 'sonner'
import type { AgentSessionHeader } from '@/lib/bindings'
import { formatCompactAge, sourceKindLabel } from '@/lib/agentSessionGrouping'
import { cn } from '@/lib/utils'
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuLabel,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from '@/components/ui/context-menu'
import { Input } from '@/components/ui/input'
import { ProviderGlyph, providerLogo } from '@/lib/brandLogos'

/** One session row's height, per tasks.md 3.1's stated 28-32px range. */
export const SESSION_ROW_HEIGHT = 28

/**
 * One line in the session sidebar: leading kind icon, ellipsised title, and a
 * trailing slot that is exactly one of a timestamp, a working dot, or a
 * needs-you dot -- the mockup's row anatomy (`.ag-thread`, `15px
 * minmax(0,1fr) auto` grid) establishes that the trailing slot never shows
 * two things at once, so unread/working/needs-you states extend that same
 * one-slot mechanic instead of adding a second badge column.
 *
 * Precedence when a session is both unread and working (e.g. it produced
 * output while the user was elsewhere and is still running): working wins,
 * because it is the more time-sensitive fact -- unread is still visible via
 * the row's bold title treatment, matching how mail/chat clients keep an
 * unread indicator separate from a live-activity indicator.
 */
export function SessionRow({
  header,
  selected,
  onSelect,
  onRename,
  onArchive,
  onDelete,
  style,
  showProject = false,
}: {
  header: AgentSessionHeader
  /**
   * Show which project this chat belongs to. Off when the group header
   * already says it (Project grouping); on everywhere else, including the
   * default Recent grouping, where the row otherwise gives no clue which of
   * several open repositories a chat would act on.
   */
  showProject?: boolean
  selected: boolean
  onSelect: (sessionId: string) => void
  onRename: (sessionId: string, title: string) => void
  onArchive: (sessionId: string, archived: boolean) => void
  /** Asks to delete this chat. The confirm lives with the caller. */
  onDelete: (sessionId: string, title: string) => void
  /** Positioning style from the virtualizer; applied directly to the row element. */
  style?: React.CSSProperties
}) {
  const [renaming, setRenaming] = useState(false)
  const [draftTitle, setDraftTitle] = useState(header.title)

  const title = header.title.trim() || 'Untitled chat'
  const needsYou = header.state === 'needsInput'
  const working = header.state === 'working'
  // A run that died mid-flight used to fall through to a plain grey
  // timestamp, so a chat killed by a crash -- still holding changed files --
  // looked exactly like one that finished cleanly. Returning after a crash is
  // precisely when a person scans this list for what needs them.
  const stoppedBadly =
    header.state === 'failed' || header.state === 'interrupted' || header.state === 'missingSource'

  const commitRename = () => {
    setRenaming(false)
    const trimmed = draftTitle.trim()
    if (trimmed && trimmed !== header.title) {
      onRename(header.sessionId, trimmed)
    } else {
      setDraftTitle(header.title)
    }
  }

  if (renaming) {
    return (
      <div style={style} className="flex items-center px-1.5" data-row-height={SESSION_ROW_HEIGHT}>
        <Input
          autoFocus
          value={draftTitle}
          onChange={(e) => setDraftTitle(e.target.value)}
          onBlur={commitRename}
          onKeyDown={(e) => {
            if (e.key === 'Enter') {
              e.preventDefault()
              commitRename()
            } else if (e.key === 'Escape') {
              e.preventDefault()
              setDraftTitle(header.title)
              setRenaming(false)
            }
          }}
          className="h-6 text-2xs"
        />
      </div>
    )
  }

  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>
        <button
          type="button"
          style={style}
          onClick={() => onSelect(header.sessionId)}
          title={title}
          aria-current={selected ? 'true' : undefined}
          className={cn(
            'grid w-full min-w-0 items-center gap-1.5 rounded px-1.5 text-left text-2xs',
            'grid-cols-[15px_minmax(0,1fr)_auto]',
            // A soft fill PLUS a persistent edge, per the Navigation rule --
            // the fill alone is close to invisible on a dim screen.
            'border-l-2',
            selected
              ? 'border-primary bg-soft text-foreground'
              : 'border-transparent text-sub hover:bg-panel2 hover:text-foreground'
          )}
        >
          <span
            className={cn('flex h-[15px] w-[15px] items-center justify-center', selected ? 'text-accent-text' : 'text-muted-foreground')}
            aria-label={header.preferredProvider ? `Uses ${header.preferredProvider}` : 'Uses the default AI'}
            title={`${header.preferredProvider ?? 'Default AI'} · Started from ${sourceKindLabel(header.source.kind)}`}
          >
            {header.preferredProvider && providerLogo(header.preferredProvider) ? (
              <ProviderGlyph id={header.preferredProvider} size={13} />
            ) : (
              <Bot size={13} aria-hidden />
            )}
          </span>
          <span
            className={cn(
              'min-w-0 overflow-hidden text-ellipsis whitespace-nowrap',
              header.unread && !selected && 'font-semibold text-foreground'
            )}
          >
            {title}
            {showProject && header.repoName ? (
              <span className="ml-1.5 font-normal text-muted-foreground">{header.repoName}</span>
            ) : null}
          </span>
          <span className="flex-none">
            {working ? (
              <span
                className="block h-1.5 w-1.5 rounded-full bg-[var(--gw-blue)] shadow-[0_0_0_3px_color-mix(in_srgb,var(--gw-blue)_15%,transparent)]"
                role="status"
                aria-label="Working"
              />
            ) : needsYou ? (
              <span
                className="block h-1.5 w-1.5 rounded-full bg-[var(--gw-amber)] shadow-[0_0_0_3px_color-mix(in_srgb,var(--gw-amber)_18%,transparent)]"
                role="status"
                aria-label="Needs your input"
              />
            ) : stoppedBadly ? (
              <span
                className="block h-1.5 w-1.5 rounded-full bg-[var(--gw-red)] shadow-[0_0_0_3px_color-mix(in_srgb,var(--gw-red)_18%,transparent)]"
                role="status"
                aria-label="Stopped before finishing"
              />
            ) : (
              <span className="font-mono text-[10px] text-muted-foreground">
                {formatCompactAge(header.updatedAt)}
              </span>
            )}
          </span>
        </button>
      </ContextMenuTrigger>
      <ContextMenuContent className="w-52">
        <ContextMenuLabel className="overflow-hidden text-ellipsis whitespace-nowrap text-2xs text-sub">
          {title}
        </ContextMenuLabel>
        <ContextMenuSeparator />
        <ContextMenuItem
          onSelect={() => {
            setDraftTitle(header.title)
            setRenaming(true)
          }}
        >
          <Pencil />
          Rename
        </ContextMenuItem>
        {header.archived ? (
          <ContextMenuItem
            onSelect={() => {
              onArchive(header.sessionId, false)
              toast.success(`Restored "${title}".`)
            }}
          >
            <ArchiveRestore />
            Restore
          </ContextMenuItem>
        ) : (
          <ContextMenuItem
            onSelect={() => {
              onArchive(header.sessionId, true)
              toast.success(`Archived "${title}".`, {
                description: 'You can restore it from the Archived filter any time.',
              })
            }}
          >
            <Archive />
            Archive
          </ContextMenuItem>
        )}
        <ContextMenuSeparator />
        {/* Separated from Archive on purpose: archive is for a chat you are
            done with, delete is for one that should not exist. Putting them
            adjacent without a divider is how someone loses work reaching for
            the other one. */}
        <ContextMenuItem
          variant="destructive"
          onSelect={() => onDelete(header.sessionId, title)}
        >
          <Trash2 />
          Delete
        </ContextMenuItem>
      </ContextMenuContent>
    </ContextMenu>
  )
}
