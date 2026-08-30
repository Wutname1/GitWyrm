import { useState } from 'react'
import { toast } from 'sonner'
import { Download, ExternalLink, FolderOpen } from 'lucide-react'
import type { AdapterListEntry, ScannedExternalSession } from '@/lib/bindings'
import {
  useAgentImportAdapters,
  useAgentImportContinuation,
  useAgentImportScan,
  useContinueImportedSessionHere,
  useImportExternalSession,
} from '@/hooks/useAgentImport'
import { canBrowseAdapter, continueExternallyLabel, detectionLabel, projectLabel } from '@/lib/agentImportDisplay'
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

      {selectedAdapterId && (
        <SessionList
          adapterId={selectedAdapterId}
          enabled={
            adapters.data?.find((a) => a.adapterId === selectedAdapterId)?.enabled ?? false
          }
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

function SessionList({ adapterId, enabled }: { adapterId: string; enabled: boolean }) {
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
        <SessionRow key={s.summary.externalSessionId} adapterId={adapterId} session={s} />
      ))}
    </div>
  )
}

function SessionRow({ adapterId, session }: { adapterId: string; session: ScannedExternalSession }) {
  const importMutation = useImportExternalSession()
  const continueHereMutation = useContinueImportedSessionHere()
  const continuation = useAgentImportContinuation(adapterId, session.summary.externalSessionId)
  const project = projectLabel(session)

  const handleImport = () => {
    importMutation.mutate(
      { adapterId, externalSessionId: session.summary.externalSessionId },
      {
        onSuccess: (result) => {
          if (result.kind === 'created' || result.kind === 'refreshed') {
            toast.success(`Imported "${session.summary.title}"`)
          } else {
            toast.error(`Could not import: ${result.kind}`)
          }
        },
        onError: (error) => {
          log.error(`import session failed: ${String(error)}`)
          toast.error('Could not import this session')
        },
      }
    )
  }

  const handleContinueHere = () => {
    if (importMutation.data?.kind !== 'created' && importMutation.data?.kind !== 'refreshed') return
    const sessionId = importMutation.data.session.header.sessionId
    continueHereMutation.mutate(sessionId, {
      onSuccess: (result) => {
        if (result.kind === 'continued') {
          toast.success('Continuing this chat in GitWyrm')
        } else {
          toast.error('Could not continue this chat')
        }
      },
    })
  }

  // "Continue session" is only ever offered when the adapter can genuinely
  // resume this exact session; everything else is "Open client" so the copy
  // never claims context transfer that did not happen (spec: "Continuation
  // is honest").
  const continueExternalLabel = continueExternallyLabel(continuation.data)

  return (
    <div className="flex flex-col gap-1 rounded-md border border-border px-2.5 py-2 text-xs">
      <div className="flex items-center justify-between gap-2">
        <span className="truncate font-medium text-foreground" title={session.summary.title}>
          {session.summary.title}
        </span>
        {session.alreadyImported && (
          <span className="inline-flex items-center gap-1 rounded-full bg-panel3 px-1.5 py-px text-[9.5px] font-semibold uppercase tracking-wide text-muted-foreground">
            <Download size={9} aria-hidden />
            Imported
          </span>
        )}
      </div>
      <span className={cn('text-[11px]', project.resolved ? 'text-muted-foreground' : 'text-[var(--gw-amber)]')}>
        {project.text}
      </span>
      <div className="mt-1 flex items-center gap-2">
        <button
          type="button"
          onClick={handleImport}
          disabled={importMutation.isPending}
          className="rounded-md bg-accent px-2 py-1 text-[11px] font-medium text-accent-text hover:bg-accent-hover disabled:opacity-60"
        >
          {session.alreadyImported ? 'Refresh' : 'Import'}
        </button>
        {(importMutation.data?.kind === 'created' || importMutation.data?.kind === 'refreshed') && (
          <button
            type="button"
            onClick={handleContinueHere}
            className="inline-flex items-center gap-1 rounded-md border border-border px-2 py-1 text-[11px] font-medium text-foreground hover:bg-panel2"
          >
            Continue here
          </button>
        )}
        {continueExternalLabel && (
          <span className="inline-flex items-center gap-1 text-[11px] text-muted-foreground">
            <ExternalLink size={10} aria-hidden />
            {continueExternalLabel}
          </span>
        )}
        {project.offerLinking && (
          <span className="inline-flex items-center gap-1 text-[11px] text-muted-foreground">
            <FolderOpen size={10} aria-hidden />
            Add this folder as a repo to link it
          </span>
        )}
      </div>
    </div>
  )
}
