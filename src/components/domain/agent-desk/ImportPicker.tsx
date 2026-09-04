import { useState } from 'react'
import { toast } from 'sonner'
import { Download, ExternalLink, FolderOpen, Unlink } from 'lucide-react'
import type { AdapterListEntry, ScannedExternalSession } from '@/lib/bindings'
import { ConfirmDialog } from '@/components/modals/ConfirmDialog'
import {
  useAgentImportAdapters,
  useAgentImportContinuation,
  useAgentImportScan,
  useContinueImportedSessionHere,
  useImportExternalSession,
  useUnlinkImportedSession,
} from '@/hooks/useAgentImport'
import {
  canBrowseAdapter,
  continueExternallyLabel,
  detectionLabel,
  linkedImportedSessionId,
  projectLabel,
  explainImportOutcome,
  unlinkConfirmCopy,
} from '@/lib/agentImportDisplay'
import { describeError, log } from '@/lib/log'
import { cn } from '@/lib/utils'

/**
 * Browse detected external chat clients and import their sessions into
 * Agent Desk (agent-desk-external-chat-import, tasks 4.1-4.5).
 *
 * Self-contained: this file owns its own state and does not touch
 * `AgentDeskView.tsx` or `agentDeskUiStore.ts`, which other in-flight work
 * owns concurrently. A caller mounts `<ImportPicker />` wherever the "Import
 * chats" entry point ends up living.
 */
export function ImportPicker() {
  const adapters = useAgentImportAdapters()
  const [selectedAdapterId, setSelectedAdapterId] = useState<string | null>(null)
  const selectedAdapter = adapters.data?.find((a) => a.adapterId === selectedAdapterId)

  return (
    // Same flex-row reasoning as `AgentSetupView`: without `min-w-0 flex-1`
    // this would draw over the chat list beside it.
    <div className="flex h-full min-h-0 min-w-0 flex-1 flex-col gap-3 overflow-hidden p-3">
      <div>
        <h2 className="text-sm font-semibold text-foreground">Import chats</h2>
        <p className="text-xs text-muted-foreground">
          Bring sessions in from other chat tools without changing anything there.
        </p>
      </div>

      {adapters.isLoading && <p className="text-xs text-muted-foreground">Looking for chat tools…</p>}
      {adapters.isError && (
        <p className="text-xs text-[var(--gw-red)]">Could not check for chat tools: {describeError(adapters.error)}</p>
      )}

      <div className="flex flex-col gap-1.5 overflow-y-auto">
        {adapters.data?.map((entry) => (
          <AdapterRow
            key={entry.adapterId}
            entry={entry}
            selected={selectedAdapterId === entry.adapterId}
            onSelect={() => setSelectedAdapterId(entry.adapterId)}
          />
        ))}
      </div>

      {selectedAdapter && (
        <SessionList
          adapterId={selectedAdapter.adapterId}
          adapterName={selectedAdapter.displayName}
          enabled={selectedAdapter.enabled}
        />
      )}
    </div>
  )
}

function AdapterRow({
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
      className={cn(
        'flex items-center justify-between rounded-md border border-border px-2.5 py-2 text-left text-xs transition-colors',
        canBrowse ? 'hover:bg-panel2 cursor-pointer' : 'cursor-not-allowed opacity-60',
        selected && 'border-accent bg-panel2'
      )}
    >
      <span className="font-medium text-foreground">{entry.displayName}</span>
      <span className="text-muted-foreground">{detectionLabel(entry)}</span>
    </button>
  )
}

function SessionList({
  adapterId,
  adapterName,
  enabled,
}: {
  adapterId: string
  adapterName: string
  enabled: boolean
}) {
  const scan = useAgentImportScan(adapterId, enabled)

  if (!enabled) {
    return (
      <p className="text-xs text-muted-foreground">
        This chat tool was found, but importing from it is not supported yet.
      </p>
    )
  }
  if (scan.isLoading) {
    return <p className="text-xs text-muted-foreground">Looking for sessions…</p>
  }
  if (scan.isError) {
    return (
      <p className="text-xs text-[var(--gw-red)]">Could not list sessions: {describeError(scan.error)}</p>
    )
  }
  if (!scan.data || scan.data.kind !== 'scanned') {
    return <p className="text-xs text-muted-foreground">No sessions available right now.</p>
  }
  if (scan.data.sessions.length === 0) {
    return <p className="text-xs text-muted-foreground">No sessions found for this chat tool.</p>
  }

  return (
    <div className="flex flex-col gap-1.5 overflow-y-auto">
      {scan.data.sessions.map((s) => (
        <SessionRow
          key={s.summary.externalSessionId}
          adapterId={adapterId}
          adapterName={adapterName}
          session={s}
        />
      ))}
    </div>
  )
}

function SessionRow({
  adapterId,
  adapterName,
  session,
}: {
  adapterId: string
  adapterName: string
  session: ScannedExternalSession
}) {
  const externalSessionId = session.summary.externalSessionId
  const importMutation = useImportExternalSession()
  const continueHereMutation = useContinueImportedSessionHere()
  const unlinkMutation = useUnlinkImportedSession()
  const [confirmUnlinkOpen, setConfirmUnlinkOpen] = useState(false)

  // One source of truth for "is this row tied to a GitWyrm chat right now".
  // Every action that needs a GitWyrm session (Continue here, Open client,
  // Unlink) hangs off this, so after Unlink they all disappear together.
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
        onError: (error) => {
          log.error(`import session failed: ${String(error)}`)
          toast.error('Could not import this session')
        },
      }
    )
  }

  const handleContinueHere = () => {
    if (!linkedSessionId) return
    continueHereMutation.mutate(linkedSessionId, {
      onSuccess: (result) => {
        if (result.kind === 'continued') {
          toast.success('Continuing this chat in GitWyrm')
        } else {
          toast.error('Could not continue this chat')
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
              // Drop this row's own import result so the linked-only actions
              // hide immediately instead of waiting for the scan refetch.
              importMutation.reset()
              toast.success(`Unlinked from ${result.adapterDisplayName}. The imported messages stay in GitWyrm.`)
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
        onError: (error) => {
          log.error(`unlink failed: ${String(error)}`)
          toast.error('Could not unlink this chat')
        },
      }
    )
  }

  // "Continue session" is only ever offered when the adapter can genuinely
  // resume this exact session; everything else is "Open client" so the copy
  // never claims context transfer that did not happen (spec: "Continuation
  // is honest").
  const continueExternalLabel = linkedSessionId ? continueExternallyLabel(continuation.data) : null
  const unlinkCopy = unlinkConfirmCopy(adapterName)

  return (
    <div className="flex flex-col gap-1 rounded-md border border-border px-2.5 py-2 text-xs">
      <div className="flex items-center justify-between gap-2">
        <span className="truncate font-medium text-foreground" title={session.summary.title}>
          {session.summary.title}
        </span>
        {linkedSessionId && (
          <span className="inline-flex items-center gap-1 rounded-full bg-panel3 px-1.5 py-px text-[9.5px] font-semibold uppercase tracking-wide text-muted-foreground">
            <Download size={9} aria-hidden />
            Imported
          </span>
        )}
      </div>
      <span className={cn('text-2xs', project.resolved ? 'text-muted-foreground' : 'text-[var(--gw-amber)]')}>
        {project.text}
      </span>
      <div className="mt-1 flex flex-wrap items-center gap-2">
        <button
          type="button"
          onClick={handleImport}
          disabled={importMutation.isPending}
          className="rounded-md bg-accent px-2 py-1 text-2xs font-medium text-accent-text hover:bg-accent-hover disabled:opacity-60"
        >
          {linkedSessionId ? 'Refresh' : 'Import'}
        </button>
        {linkedSessionId && (
          <button
            type="button"
            onClick={handleContinueHere}
            className="inline-flex items-center gap-1 rounded-md border border-border px-2 py-1 text-2xs font-medium text-foreground hover:bg-panel2"
          >
            Continue here
          </button>
        )}
        {continueExternalLabel && (
          <span className="inline-flex items-center gap-1 text-2xs text-muted-foreground">
            <ExternalLink size={10} aria-hidden />
            {continueExternalLabel}
          </span>
        )}
        {linkedSessionId && (
          <button
            type="button"
            onClick={() => setConfirmUnlinkOpen(true)}
            disabled={unlinkMutation.isPending}
            className="inline-flex items-center gap-1 rounded-md border border-border px-2 py-1 text-2xs font-medium text-muted-foreground hover:bg-panel2 hover:text-foreground disabled:opacity-60"
          >
            <Unlink size={10} aria-hidden />
            Unlink from {adapterName}
          </button>
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
        pending={unlinkMutation.isPending}
        pendingLabel="Unlinking…"
        onConfirm={handleUnlink}
      />
    </div>
  )
}
