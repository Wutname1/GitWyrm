import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { toast } from 'sonner'
import {
  Check,
  Download,
  ExternalLink,
  FolderOpen,
  RefreshCw,
  Search,
  Unlink,
  X,
} from 'lucide-react'
import type { AdapterListEntry, ScannedExternalSession } from '@/lib/bindings'
import { ConfirmDialog } from '@/components/modals/ConfirmDialog'
import { PendingIndicator } from '@/components/ui/pending-indicator'
import { ImportList } from '@/components/domain/agent-desk/ImportList'
import {
  useAgentImportAdapters,
  useAgentImportContinuation,
  useAgentImportScan,
  useAgentImportSyncPreferences,
  useContinueImportedSessionHere,
  useImportExternalSession,
  useImportExternalSessionBatch,
  useSetImportSyncPreference,
  useUnlinkImportedSession,
} from '@/hooks/useAgentImport'
import {
  buildImportRows,
  filterImportSessions,
  nextSelection,
  projectChoices,
  summarizeSelection,
  toggleSelectAllVisible,
  type ImportedFilter,
  type ProjectFilter,
  type SelectModifiers,
} from '@/lib/agentImportList'
import {
  SYNC_POLL_MS,
  canBrowseAdapter,
  continueExternallyLabel,
  detectionLabel,
  explainBatchRefusal,
  explainImportOutcome,
  explainImportScanRefusal,
  explainSyncImport,
  linkedImportedSessionId,
  projectLabel,
  summarizeBatchImport,
  syncToggleCopy,
  unlinkConfirmCopy,
} from '@/lib/agentImportDisplay'
import { describeError, log } from '@/lib/log'
import { cn } from '@/lib/utils'

/**
 * Browse detected external chat clients and bring their chats into Agent Desk.
 *
 * Rebuilt for real installations. The first version rendered every scanned
 * chat as a card with its own three mutation hooks and its own capability
 * probe, which is fine for the eight chats a fixture has and unusable for the
 * several hundred a year of Claude Code leaves on disk: hundreds of file reads
 * fired on open, and importing them meant pressing Import hundreds of times.
 *
 * The shape now is a list, not a stack of cards: bulk selection with the
 * commit list's own grammar, one action bar, virtualized rows that query
 * nothing, and search/filter/day-grouping so a specific chat can be found
 * without scrolling. Per-chat actions moved into a details strip for the one
 * selected chat, which is the only place the capability probe now runs.
 */
export function ImportPicker({ onOpenSession }: { onOpenSession?: (sessionId: string) => void }) {
  const adapters = useAgentImportAdapters()
  const [selectedAdapterId, setSelectedAdapterId] = useState<string | null>(null)
  const selectedAdapter = adapters.data?.find((a) => a.adapterId === selectedAdapterId)

  return (
    // Same flex-row reasoning as `AgentSetupView`: without `min-w-0 flex-1`
    // this would draw over the chat list beside it.
    <div className="flex h-full min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
      <div className="flex-none border-b border-border px-3 py-2.5">
        <h2 className="text-sm font-semibold text-foreground">Import chats</h2>
        <p className="text-xs text-muted-foreground">
          Bring chats in from other AI tools without changing anything there.
        </p>

        {adapters.isPending && (
          <p className="mt-2 text-xs text-muted-foreground">Looking for AI tools…</p>
        )}
        {adapters.isError && (
          <p className="mt-2 text-xs text-[var(--gw-red)]">
            Could not check for AI tools: {describeError(adapters.error)}
          </p>
        )}

        {adapters.data && adapters.data.length > 0 && (
          // Wraps rather than scrolls: at the 720px minimum width five tools
          // do not fit on one line, and a row that scrolls sideways hides the
          // tool someone is looking for.
          <div className="mt-2 flex flex-wrap gap-1">
            {adapters.data.map((entry) => (
              <AdapterChip
                key={entry.adapterId}
                entry={entry}
                selected={selectedAdapterId === entry.adapterId}
                onSelect={() => setSelectedAdapterId(entry.adapterId)}
              />
            ))}
          </div>
        )}
      </div>

      {selectedAdapter ? (
        <AdapterChats
          key={selectedAdapter.adapterId}
          adapterId={selectedAdapter.adapterId}
          adapterName={selectedAdapter.displayName}
          enabled={selectedAdapter.enabled}
          onOpenSession={onOpenSession}
        />
      ) : (
        <div className="flex flex-1 items-center justify-center p-4 text-center">
          <p className="max-w-[18rem] text-xs leading-relaxed text-muted-foreground">
            {adapters.data && adapters.data.some(canBrowseAdapter)
              ? 'Pick a tool above to see the chats it has saved.'
              : adapters.isPending || adapters.isError
                ? ''
                : 'No AI tools GitWyrm can read were found on this computer.'}
          </p>
        </div>
      )}
    </div>
  )
}

function AdapterChip({
  entry,
  selected,
  onSelect,
}: {
  entry: AdapterListEntry
  selected: boolean
  onSelect: () => void
}) {
  const canBrowse = canBrowseAdapter(entry)
  return (
    <button
      type="button"
      onClick={canBrowse ? onSelect : undefined}
      disabled={!canBrowse}
      title={detectionLabel(entry)}
      className={cn(
        'flex items-center gap-1.5 rounded-[5px] border px-2 py-1 text-2xs transition-colors',
        canBrowse
          ? 'cursor-pointer hover:bg-panel2'
          : 'cursor-not-allowed border-border opacity-60',
        // `--accent` equals `--border` in this theme, so the edge alone was
        // invisible; the mint edge plus the tint is what makes selected read.
        selected && canBrowse
          ? 'border-primary bg-soft text-foreground'
          : 'border-border text-muted-foreground'
      )}
    >
      <span className="font-medium">{entry.displayName}</span>
      {!canBrowse && <span className="text-2xs">{detectionLabel(entry)}</span>}
    </button>
  )
}

function AdapterChats({
  adapterId,
  adapterName,
  enabled,
  onOpenSession,
}: {
  adapterId: string
  adapterName: string
  enabled: boolean
  onOpenSession?: (sessionId: string) => void
}) {
  const scan = useAgentImportScan(adapterId, enabled)
  const batchImport = useImportExternalSessionBatch()

  const [search, setSearch] = useState('')
  const [project, setProject] = useState<ProjectFilter>({ kind: 'all' })
  const [importedFilter, setImportedFilter] = useState<ImportedFilter>('all')
  const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(new Set())
  const [selectedIds, setSelectedIds] = useState<string[]>([])
  const [anchor, setAnchor] = useState<string | null>(null)
  const [detailsId, setDetailsId] = useState<string | null>(null)

  const sessions: ScannedExternalSession[] = useMemo(
    () => (scan.data?.kind === 'scanned' ? scan.data.sessions : []),
    [scan.data]
  )
  const visible = useMemo(
    () => filterImportSessions(sessions, { search, project, imported: importedFilter }),
    [sessions, search, project, importedFilter]
  )
  const rows = useMemo(
    () => buildImportRows(visible, { collapsedGroupIds: collapsed }),
    [visible, collapsed]
  )
  const visibleIds = useMemo(() => visible.map((s) => s.summary.externalSessionId), [visible])
  const selectedSet = useMemo(() => new Set(selectedIds), [selectedIds])
  const choices = useMemo(() => projectChoices(sessions), [sessions])
  const tally = useMemo(() => summarizeSelection(selectedIds, sessions), [selectedIds, sessions])

  // Click handling reads the current order and selection, but its identity
  // must not change on every scroll tick -- it is handed to every visible row.
  // Same ref trick the commit list uses for the same reason.
  const clickState = useRef({ visibleIds, selectedSet, anchor })
  clickState.current = { visibleIds, selectedSet, anchor }

  const handleSelect = useCallback((id: string, mods: SelectModifiers) => {
    const { visibleIds, selectedSet, anchor } = clickState.current
    const next = nextSelection(id, mods, {
      order: visibleIds,
      selected: selectedSet,
      anchor,
    })
    setSelectedIds(next.selected)
    setAnchor(next.anchor)
    // A plain click also opens that chat's details, so the per-chat actions
    // are one click away rather than hidden behind a second gesture. Modifier
    // clicks are building a selection, not asking about one chat.
    if (!mods.shift && !mods.ctrl) setDetailsId(id)
  }, [])

  const handleSelectAll = () => {
    const next = toggleSelectAllVisible(visibleIds, selectedSet)
    setSelectedIds(next.selected)
    setAnchor(next.anchor)
  }

  const toggleGroup = (groupId: string) => {
    setCollapsed((prev) => {
      const next = new Set(prev)
      if (next.has(groupId)) next.delete(groupId)
      else next.add(groupId)
      return next
    })
  }

  /**
   * Run one batch and report it. `syncedFrom` is set only when this batch was
   * started by keep-in-sync rather than by a press, so the report can say that
   * these chats arrived on their own.
   */
  const runBatch = (ids: string[], syncedFrom?: string) => {
    batchImport.mutate(
      { adapterId, externalSessionIds: ids },
      {
        onSuccess: (result) => {
          if (result.kind !== 'completed') {
            toast.error(explainBatchRefusal(result, adapterId))
            return
          }
          const summary = summarizeBatchImport(result.items, adapterId)
          const syncLine = syncedFrom ? explainSyncImport(summary, syncedFrom) : null
          const headline = syncLine ?? summary.message
          // A reason, not just a tally. "3 could not be brought in" with
          // nothing else leaves the person no idea whether a file is damaged
          // or the tool has gone -- and the reasons are usually all the same
          // one, so the first names the problem for the group.
          const detail =
            summary.failed.length === 0
              ? undefined
              : summary.failed.length === 1
                ? summary.failed[0]?.reason
                : `${summary.failed[0]?.reason} (and ${summary.failed.length - 1} more)`

          if (summary.ok) toast.success(headline)
          else toast.error(headline, { description: detail })

          if (summary.failed.length > 0) {
            log.error(
              `batch import from ${adapterId}: ${summary.failed.map((f) => f.reason).join('; ')}`
            )
          }

          // Only a batch the person pressed for clears their selection. Sync
          // runs on a timer and must never reach in and discard chats someone
          // is in the middle of picking.
          if (!syncedFrom) {
            setSelectedIds([])
            setAnchor(null)
          }
        },
      }
    )
  }

  const handleImportSelected = () => {
    if (selectedIds.length === 0) return
    runBatch(selectedIds)
  }

  useImportSync({
    adapterId,
    enabled,
    sessions,
    scanning: scan.isFetching,
    importing: batchImport.isPending,
    // Sync brings chats in without being asked, so it reports every time it
    // does -- silently growing the chat list is exactly what an opt-in is
    // supposed to rule out.
    onImport: (ids) => runBatch(ids, adapterName),
  })

  if (!enabled) {
    return (
      <Notice>
        GitWyrm found {adapterName}, but cannot read its chats yet.
      </Notice>
    )
  }
  if (scan.isPending) {
    return <Notice>Looking for chats…</Notice>
  }
  if (scan.isError) {
    return (
      <Notice tone="error">Could not list chats: {describeError(scan.error)}</Notice>
    )
  }
  if (!scan.data || scan.data.kind !== 'scanned') {
    // "No chats found" for all of these would read as a statement about the
    // person's work, when the usual truth is that GitWyrm could not look.
    return <Notice tone="warn">{scan.data ? explainImportScanRefusal(scan.data) : ''}</Notice>
  }
  if (sessions.length === 0) {
    return <Notice>GitWyrm looked and {adapterName} has no saved chats.</Notice>
  }

  const detailsSession = detailsId
    ? sessions.find((s) => s.summary.externalSessionId === detailsId)
    : undefined
  const allVisibleSelected = visibleIds.length > 0 && visibleIds.every((id) => selectedSet.has(id))

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex-none border-b border-border px-2 py-1.5">
        {/* Wraps at the 720px minimum so search keeps a usable width instead of
            the filters pushing it to nothing. */}
        <div className="flex flex-wrap items-center gap-1.5">
          <label className="relative flex h-7 min-w-[10rem] flex-1 items-center">
            <Search
              size={12}
              aria-hidden
              className="pointer-events-none absolute left-2 text-muted-foreground"
            />
            <input
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              placeholder="Search chats"
              aria-label="Search chats"
              className={cn(
                'h-7 w-full rounded-[5px] border border-border bg-canvas pl-6 pr-6 text-xs text-foreground',
                'placeholder:text-muted-foreground outline-none transition-colors',
                'focus-visible:border-primary focus-visible:ring-[3px] focus-visible:ring-primary/40'
              )}
            />
            {search && (
              <button
                type="button"
                onClick={() => setSearch('')}
                aria-label="Clear search"
                className="absolute right-1.5 text-muted-foreground hover:text-foreground"
              >
                <X size={11} />
              </button>
            )}
          </label>

          <select
            value={project.kind === 'path' ? project.path : project.kind}
            onChange={(e) => {
              const v = e.target.value
              if (v === 'all') setProject({ kind: 'all' })
              else if (v === 'unresolved') setProject({ kind: 'unresolved' })
              else setProject({ kind: 'path', path: v })
            }}
            aria-label="Show chats from"
            className={cn(
              'h-7 max-w-[11rem] rounded-[5px] border border-border bg-canvas px-1.5 text-xs text-foreground',
              'outline-none transition-colors',
              'focus-visible:border-primary focus-visible:ring-[3px] focus-visible:ring-primary/40',
              project.kind !== 'all' && 'border-primary'
            )}
          >
            <option value="all">All projects</option>
            {choices.some((c) => c.unresolved) && (
              <option value="unresolved">Folder not found</option>
            )}
            {choices.map((c) => (
              <option key={c.path} value={c.path}>
                {c.name} ({c.count})
              </option>
            ))}
          </select>

          <button
            type="button"
            onClick={() => setImportedFilter((f) => (f === 'all' ? 'notImported' : 'all'))}
            aria-pressed={importedFilter === 'notImported'}
            className={cn(
              'h-7 rounded-[5px] border px-2 text-2xs transition-colors',
              importedFilter === 'notImported'
                ? 'border-primary bg-soft text-foreground'
                : 'border-border text-muted-foreground hover:bg-panel2 hover:text-foreground'
            )}
          >
            Not in yet
          </button>

          <button
            type="button"
            onClick={() => void scan.refetch()}
            disabled={scan.isFetching}
            aria-label="Check for new chats"
            title="Check for new chats"
            className={cn(
              'flex h-7 w-7 flex-none items-center justify-center rounded-[5px] border border-border',
              'text-muted-foreground transition-colors hover:bg-panel2 hover:text-foreground',
              'disabled:opacity-60'
            )}
          >
            {scan.isFetching ? (
              <PendingIndicator className="size-3" />
            ) : (
              <RefreshCw size={12} />
            )}
          </button>
        </div>

        <div className="mt-1.5 flex flex-wrap items-center justify-between gap-2">
          <button
            type="button"
            onClick={handleSelectAll}
            disabled={visibleIds.length === 0}
            className={cn(
              'flex items-center gap-1.5 text-2xs text-muted-foreground transition-colors',
              'hover:text-foreground disabled:opacity-60'
            )}
          >
            <span
              aria-hidden
              className={cn(
                'flex size-3.5 items-center justify-center rounded-[3px] border transition-colors',
                allVisibleSelected
                  ? 'border-primary bg-primary text-primary-foreground'
                  : 'border-border'
              )}
            >
              {allVisibleSelected && <Check size={10} strokeWidth={3} />}
            </span>
            {allVisibleSelected ? 'Clear these' : `Pick all ${visible.length}`}
          </button>

          <SyncToggle adapterId={adapterId} adapterName={adapterName} />
        </div>
      </div>

      {visible.length === 0 ? (
        <div className="flex flex-1 items-center justify-center p-4 text-center">
          <p className="max-w-[18rem] text-xs leading-relaxed text-muted-foreground">
            {/* Names the filter that is hiding them, so the way back is
                obvious rather than reading as "you have no chats". */}
            No chats match what you are looking for. {adapterName} has{' '}
            {sessions.length} saved in total.
          </p>
        </div>
      ) : (
        <ImportList
          rows={rows}
          selected={selectedSet}
          activeId={detailsId}
          onSelect={handleSelect}
          onToggleGroup={toggleGroup}
          onOpenDetails={setDetailsId}
        />
      )}

      {detailsSession && (
        <ChatDetails
          adapterId={adapterId}
          adapterName={adapterName}
          session={detailsSession}
          onClose={() => setDetailsId(null)}
          onOpenSession={onOpenSession}
        />
      )}

      {selectedIds.length > 0 && (
        <div className="flex flex-none items-center justify-between gap-2 border-t border-border bg-panel2 px-2.5 py-2">
          <span className="min-w-0 truncate text-2xs text-muted-foreground">
            {/* Says what the press will actually do. "Import 24 chats" when 19
                are refreshes describes work it is not doing. */}
            {tally.newChats > 0 && `${tally.newChats} new`}
            {tally.newChats > 0 && tally.refreshes > 0 && ', '}
            {tally.refreshes > 0 && `${tally.refreshes} already in, will update`}
          </span>
          <div className="flex flex-none items-center gap-1.5">
            <button
              type="button"
              onClick={() => {
                setSelectedIds([])
                setAnchor(null)
              }}
              className="rounded-[5px] border border-border px-2 py-1 text-2xs text-muted-foreground transition-colors hover:bg-panel3 hover:text-foreground"
            >
              Clear
            </button>
            <button
              type="button"
              onClick={handleImportSelected}
              disabled={batchImport.isPending}
              className={cn(
                'inline-flex items-center gap-1.5 rounded-md bg-primary px-2.5 py-1 text-2xs font-medium',
                'text-primary-foreground transition-colors hover:bg-primary/90 disabled:opacity-60'
              )}
            >
              {batchImport.isPending && <PendingIndicator className="size-3" />}
              {batchImport.isPending
                ? 'Bringing them in…'
                : `Bring in ${tally.total} ${tally.total === 1 ? 'chat' : 'chats'}`}
            </button>
          </div>
        </div>
      )}
    </div>
  )
}

function Notice({
  children,
  tone = 'muted',
}: {
  children: React.ReactNode
  tone?: 'muted' | 'warn' | 'error'
}) {
  return (
    <div className="flex flex-1 items-center justify-center p-4 text-center">
      <p
        className={cn(
          'max-w-[20rem] text-xs leading-relaxed',
          tone === 'error'
            ? 'text-[var(--gw-red)]'
            : tone === 'warn'
              ? 'text-[var(--gw-amber)]'
              : 'text-muted-foreground'
        )}
      >
        {children}
      </p>
    </div>
  )
}

/**
 * Opt-in keep-in-sync for one tool.
 *
 * Off until turned on, every time, and the description says what turning it on
 * actually does before the switch is flipped -- GitWyrm reads that tool's
 * saved conversations on a timer and copies new ones in with nobody present.
 * Reading another application's files unattended is not a default.
 */
function SyncToggle({ adapterId, adapterName }: { adapterId: string; adapterName: string }) {
  const prefs = useAgentImportSyncPreferences()
  const setPref = useSetImportSyncPreference()
  const copy = syncToggleCopy(adapterName)

  const on = prefs.data?.some((p) => p.adapterId === adapterId && p.enabled) ?? false
  // A failed read is not "off". Saying "Keep in sync" with the switch dark
  // while GitWyrm does not actually know would be a confident wrong answer.
  const unknown = prefs.isError

  return (
    <button
      type="button"
      role="switch"
      aria-checked={on}
      aria-label={copy.label}
      title={unknown ? 'GitWyrm could not read this setting.' : copy.description}
      disabled={setPref.isPending || prefs.isPending || unknown}
      onClick={() => setPref.mutate({ adapterId, enabled: !on })}
      className={cn(
        'flex items-center gap-1.5 rounded-[5px] border px-1.5 py-1 text-2xs transition-colors',
        'disabled:opacity-60',
        on
          ? 'border-primary bg-soft text-foreground'
          : 'border-border text-muted-foreground hover:bg-panel2 hover:text-foreground'
      )}
    >
      {setPref.isPending ? (
        <PendingIndicator className="size-3" />
      ) : (
        <span
          aria-hidden
          className={cn(
            'flex h-3 w-5 flex-none items-center rounded-full px-px transition-colors',
            on ? 'justify-end bg-primary' : 'justify-start bg-panel3'
          )}
        >
          <span className="size-2.5 rounded-full bg-foreground" />
        </span>
      )}
      {unknown ? 'Keep in sync unavailable' : copy.label}
    </button>
  )
}

/**
 * The per-chat actions, for the one chat whose details are open.
 *
 * This is the only place `useAgentImportContinuation` runs. It was on every
 * row before, so a scan of 400 chats fired 400 capability probes -- each of
 * which opens a file -- before anyone had asked about a single one.
 */
function ChatDetails({
  adapterId,
  adapterName,
  session,
  onClose,
  onOpenSession,
}: {
  adapterId: string
  adapterName: string
  session: ScannedExternalSession
  onClose: () => void
  onOpenSession?: (sessionId: string) => void
}) {
  const externalSessionId = session.summary.externalSessionId
  const importMutation = useImportExternalSession()
  const continueHereMutation = useContinueImportedSessionHere()
  const unlinkMutation = useUnlinkImportedSession()
  const [confirmUnlinkOpen, setConfirmUnlinkOpen] = useState(false)

  // One source of truth for "is this chat tied to a GitWyrm chat right now".
  // Every action needing a GitWyrm session hangs off this, so after Unlink
  // they all disappear together.
  const linkedSessionId = linkedImportedSessionId(session, importMutation.data)
  const continuation = useAgentImportContinuation(
    linkedSessionId ? adapterId : null,
    linkedSessionId ? externalSessionId : null
  )
  const project = projectLabel(session)

  const handleImport = () => {
    importMutation.mutate(
      { adapterId, externalSessionId },
      {
        onSuccess: (result) => {
          const { message, ok } = explainImportOutcome(result, session.summary.title, adapterId)
          if (ok) toast.success(message)
          else toast.error(message)
        },
      }
    )
  }

  const handleContinueHere = () => {
    if (!linkedSessionId) return
    continueHereMutation.mutate(linkedSessionId, {
      onSuccess: (result) => {
        switch (result.kind) {
          case 'continued':
            toast.success('Continuing this chat in GitWyrm')
            // Land in the chat. Without this the person is told the chat is
            // continuing and left looking at the import list, with nothing
            // saying where it went.
            onOpenSession?.(result.session.header.sessionId)
            break
          case 'notFound':
            toast.error('That chat is no longer here.', {
              description: 'It may have been deleted since this list was loaded.',
            })
            break
          case 'writeFailed':
            toast.error('That chat could not be saved.', {
              description: `${result.detail} Nothing was changed.`,
            })
            break
        }
      },
    })
  }

  const handleUnlink = () => {
    if (!linkedSessionId) return
    unlinkMutation.mutate(
      { sessionId: linkedSessionId, adapterId, externalSessionId },
      {
        onSuccess: (result) => {
          switch (result.kind) {
            case 'unlinked':
              importMutation.reset()
              toast.success(
                `Unlinked from ${result.adapterDisplayName}. The imported messages stay in GitWyrm.`
              )
              break
            case 'notLinked':
              importMutation.reset()
              toast.info('This chat was already unlinked')
              break
            case 'notFound':
              importMutation.reset()
              toast.error('Could not find this chat in GitWyrm')
              break
            case 'failed':
              log.error(`unlink failed: ${result.detail}`)
              toast.error('Could not unlink this chat')
              break
          }
        },
        // Closed on settle, not on success: every outcome above has already
        // said its piece, and a refusal that left the dialog up would ask the
        // person to dismiss the same news twice.
        onSettled: () => setConfirmUnlinkOpen(false),
      }
    )
  }

  // Only ever offered when the adapter can genuinely resume this exact chat;
  // everything else says only that it can be continued in its own app, so the
  // copy never claims context transfer that did not happen.
  const continueExternalLabel = linkedSessionId ? continueExternallyLabel(continuation.data) : null
  const unlinkCopy = unlinkConfirmCopy(adapterName)

  return (
    <div className="flex-none border-t border-border bg-panel2 px-2.5 py-2">
      <div className="flex items-start justify-between gap-2">
        <div className="min-w-0">
          <p className="truncate text-xs font-medium text-foreground" title={session.summary.title}>
            {session.summary.title}
          </p>
          <p
            className={cn(
              'truncate text-2xs',
              project.resolved ? 'text-muted-foreground' : 'text-[var(--gw-amber)]'
            )}
          >
            {project.text} · {session.summary.messageCount} messages · from {adapterName}
          </p>
        </div>
        <button
          type="button"
          onClick={onClose}
          aria-label="Close chat details"
          className="flex-none rounded-[4px] p-0.5 text-muted-foreground transition-colors hover:bg-panel3 hover:text-foreground"
        >
          <X size={12} />
        </button>
      </div>

      <div className="mt-1.5 flex flex-wrap items-center gap-1.5">
        <button
          type="button"
          onClick={handleImport}
          disabled={importMutation.isPending}
          className="inline-flex items-center gap-1 rounded-md bg-primary px-2 py-1 text-2xs font-medium text-primary-foreground transition-colors hover:bg-primary/90 disabled:opacity-60"
        >
          {importMutation.isPending && <PendingIndicator className="size-3" />}
          {importMutation.isPending
            ? linkedSessionId
              ? 'Checking for new messages…'
              : 'Bringing it in…'
            : linkedSessionId
              ? 'Check for new messages'
              : 'Bring it in'}
        </button>
        {linkedSessionId && (
          <button
            type="button"
            onClick={handleContinueHere}
            disabled={continueHereMutation.isPending}
            className="inline-flex items-center gap-1 rounded-md border border-border px-2 py-1 text-2xs font-medium text-foreground transition-colors hover:bg-panel3 disabled:opacity-60"
          >
            {continueHereMutation.isPending && <PendingIndicator className="size-3" />}
            {continueHereMutation.isPending ? 'Opening it here…' : 'Continue here'}
          </button>
        )}
        {linkedSessionId && (
          <button
            type="button"
            onClick={() => setConfirmUnlinkOpen(true)}
            disabled={unlinkMutation.isPending}
            className="inline-flex items-center gap-1 rounded-md border border-border px-2 py-1 text-2xs font-medium text-muted-foreground transition-colors hover:bg-panel3 hover:text-foreground disabled:opacity-60"
          >
            {unlinkMutation.isPending ? (
              <PendingIndicator className="size-3" />
            ) : (
              <Unlink size={10} aria-hidden />
            )}
            {unlinkMutation.isPending ? 'Unlinking…' : `Unlink from ${adapterName}`}
          </button>
        )}
        {linkedSessionId && (
          <span className="inline-flex items-center gap-1 text-2xs text-muted-foreground">
            <Download size={10} aria-hidden />
            Already in GitWyrm
          </span>
        )}
        {continueExternalLabel && (
          // Deliberately not a button and deliberately not imperative: no
          // launch command exists, so this states where the chat can be
          // continued rather than offering to take you there.
          <span className="inline-flex items-center gap-1 text-2xs text-muted-foreground">
            <ExternalLink size={10} aria-hidden />
            {continueExternalLabel}
          </span>
        )}
        {project.offerLinking && (
          <span className="inline-flex items-center gap-1 text-2xs text-muted-foreground">
            <FolderOpen size={10} aria-hidden />
            Open this folder as a project to connect it
          </span>
        )}
      </div>

      <ConfirmDialog
        open={confirmUnlinkOpen}
        onOpenChange={setConfirmUnlinkOpen}
        title={unlinkCopy.title}
        description={unlinkCopy.description}
        confirmLabel="Unlink"
        // Held open so `pending` has something to render on. Without this the
        // dialog closed on confirm, so "Unlinking…" was never seen and a
        // second click could unlink twice.
        pending={unlinkMutation.isPending}
        pendingLabel="Unlinking…"
        keepOpenOnConfirm
        onConfirm={handleUnlink}
      />
    </div>
  )
}

/**
 * Keep-in-sync, when it is on: re-check the tool on a timer and bring in
 * whatever is new.
 *
 * A timer, not a filesystem watch, deliberately. A client's session store is a
 * directory per project, and arming a recursive watch over a tree that shape
 * is the pattern that cost this app 7.9 seconds at repo open (recorded in
 * `.context/common-pitfalls.md`). A poll every few minutes costs one scan, at
 * a moment nobody is waiting, and it stops the instant the view is closed --
 * which a watch would not.
 *
 * Only imports chats it has NOT seen before. The backend deduplicates anyway
 * (the ledger is keyed by the external chat's own id), but sending every chat
 * on every tick would make each poll a several-hundred-chat write loop to
 * discover that nothing changed.
 */
function useImportSync({
  adapterId,
  enabled,
  sessions,
  scanning,
  importing,
  onImport,
}: {
  adapterId: string
  enabled: boolean
  sessions: ScannedExternalSession[]
  scanning: boolean
  importing: boolean
  onImport: (ids: string[]) => void
}) {
  const prefs = useAgentImportSyncPreferences()
  const scan = useAgentImportScan(adapterId, enabled)
  const syncOn = prefs.data?.some((p) => p.adapterId === adapterId && p.enabled) ?? false

  // `onImport` and `refetch` are read through refs rather than listed as
  // dependencies. Both change identity on almost every render -- `onImport` is
  // built inline by the caller, and the query object is replaced on each fetch
  // -- so depending on them would tear down and rebuild the interval below
  // before it could ever fire, and re-run the import effect continuously.
  const onImportRef = useRef(onImport)
  onImportRef.current = onImport
  const refetchRef = useRef(scan.refetch)
  refetchRef.current = scan.refetch

  // What sync has already acted on, so a chat it brought in is not offered
  // again on the next tick before the scan has refetched.
  const handled = useRef<Set<string>>(new Set())
  // Chats present the first time sync was switched on are NOT imported. Turning
  // sync on means "keep up from here", not "import my entire history" -- a
  // toggle that silently pulled in four hundred chats would be the opposite of
  // the opt-in this is meant to be.
  const baselined = useRef(false)

  useEffect(() => {
    if (!syncOn) {
      baselined.current = false
      handled.current = new Set()
    }
  }, [syncOn])

  useEffect(() => {
    if (!syncOn || !enabled) return
    if (scanning || importing) return
    if (sessions.length === 0) return

    if (!baselined.current) {
      handled.current = new Set(sessions.map((s) => s.summary.externalSessionId))
      baselined.current = true
      return
    }

    const fresh = sessions
      .filter((s) => !handled.current.has(s.summary.externalSessionId))
      .map((s) => s.summary.externalSessionId)
    if (fresh.length === 0) return

    for (const id of fresh) handled.current.add(id)
    onImportRef.current(fresh)
  }, [syncOn, enabled, sessions, scanning, importing])

  // The timer only re-scans; the effect above decides what to do with what it
  // finds. Cleared whenever sync goes off or the view closes, so nothing keeps
  // reading another tool's files in the background.
  useEffect(() => {
    if (!syncOn || !enabled) return
    const id = window.setInterval(() => {
      void refetchRef.current()
    }, SYNC_POLL_MS)
    return () => window.clearInterval(id)
  }, [syncOn, enabled])
}
