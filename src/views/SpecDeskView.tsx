import { useEffect, useRef, useState } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import { commands, type RepoInfo } from '@/lib/bindings'
import { unwrap } from '@/lib/queryKeys'
import { DeskTitleBar } from '@/components/domain/spec-desk/DeskTitleBar'
import { DeskChangesList } from '@/components/domain/spec-desk/DeskChangesList'
import { DeskDetail } from '@/components/domain/spec-desk/DeskDetail'
import { DeskActionRail } from '@/components/domain/spec-desk/DeskActionRail'
import { cn } from '@/lib/utils'
import { useSpecAi } from '@/hooks/useSpecAi'
import { useOpenspecChanges, useOpenspecStatus, useSelectedChange } from '@/hooks/useOpenspec'
import { listenForSpecRefresh, listenForSpecSelection } from '@/lib/specSync'
import { readWindowMode } from '@/lib/windowMode'
import { describeError, log } from '@/lib/log'

/**
 * Opens the Desk's repository in this window's backend session.
 *
 * The Desk is a separate webview with its own empty store, so it has to open the
 * repo itself. `open_repo` reuses the handle when the path is already open, so
 * this costs nothing beyond the first call and never conflicts with the main
 * window's session.
 */
function useDeskRepo(repoPath: string | null) {
  const [repo, setRepo] = useState<RepoInfo | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    if (!repoPath) {
      setError('This window was opened without a repository.')
      return
    }
    let cancelled = false
    void (async () => {
      try {
        // Idempotent: the main window already has this repo open, so the backend
        // hands back the same handle rather than opening a second one.
        const info = unwrap(await commands.openRepo(repoPath))
        if (!cancelled) setRepo(info)
      } catch (e) {
        const message = describeError(e)
        log.error(`spec desk: could not open its repository: ${message}`)
        if (!cancelled) setError(message)
      }
    })()
    return () => {
      cancelled = true
    }
  }, [repoPath])

  return { repo, error }
}

/** Centered message for the states where there is no Desk to show. */
function DeskMessage({ title, detail }: { title: string; detail: string }) {
  return (
    <div className="flex flex-1 items-center justify-center p-8">
      <div className="max-w-sm text-center">
        <p className="text-sm font-semibold text-foreground">{title}</p>
        <p className="mt-1.5 text-xs leading-relaxed text-muted-foreground">{detail}</p>
      </div>
    </div>
  )
}

/**
 * The Spec Desk window: changes list, the selected change, and (in later
 * changes) its tabs and action rail.
 *
 * This change ships the shell. The center column shows the selected change's
 * headline state so the window is useful and obviously alive; the tabs land with
 * `add-spec-desk-detail` and the rail with `add-spec-desk-handoff-actions`.
 */
export function SpecDeskView() {
  const [mode] = useState(readWindowMode)
  const { repo, error } = useDeskRepo(mode.repoPath)
  const repoId = repo?.id ?? null
  const status = useOpenspecStatus(repoId)
  const changesQuery = useOpenspecChanges(repoId)
  const { change } = useSelectedChange(repoId)
  const changes = changesQuery.data ?? []
  const ai = useSpecAi()

  // Selections made in the main window have to move this window too.
  useEffect(() => listenForSpecSelection(), [])

  // A refresh in the main window re-reads here as well. Read through a ref so
  // the subscription survives the repo arriving: the id is null on first render
  // and resubscribing on it would drop events fired in between.
  const qc = useQueryClient()
  const repoIdRef = useRef(repoId)
  repoIdRef.current = repoId
  useEffect(() => listenForSpecRefresh(qc, () => repoIdRef.current), [qc])

  const repoName = repo?.name ?? 'Loading…'

  return (
    <div className="flex h-screen w-screen flex-col overflow-hidden bg-bg text-foreground">
      <DeskTitleBar repoName={repoName} repoId={repoId} />

      {error ? (
        <DeskMessage title="This Spec Desk has nothing to show" detail={error} />
      ) : !repo ? (
        <DeskMessage title="Opening the repository…" detail="This takes a moment on first open." />
      ) : (
        <div
          className={cn(
            'grid min-h-0 flex-1',
            // The rail only exists when there is a change to act on, so the grid
            // does not leave a third empty column.
            change
              ? 'grid-cols-[282px_minmax(0,1fr)_306px]'
              : 'grid-cols-[282px_minmax(0,1fr)]'
          )}
        >
          <DeskChangesList
            changes={changes}
            archivedCount={status.data?.archived_count ?? 0}
            selectedId={change?.id}
            repoId={repo.id}
          />

          {change ? (
            <>
              <DeskDetail change={change} repoId={repo.id} />
              <DeskActionRail change={change} repoId={repo.id} repoPath={repo.path} />
            </>
          ) : (
            <div className="p-6">
              <p className="text-xs text-muted-foreground">
                Select a change on the left to see it here.
              </p>
            </div>
          )}
        </div>
      )}

      <div className="flex h-6 flex-none items-center gap-4 border-t border-border bg-panel2 px-3 font-mono text-2xs text-muted-foreground">
        <span>Spec Desk</span>
        {change && <span>{change.id}</span>}
        <div className="flex-1" />
        {/* Omitted entirely when no AI is set up: "AI: none" is noise about
            something the user has not asked for. */}
        {ai.state === 'ready' && <span>AI: {ai.providerShort} ready</span>}
        {ai.state === 'reconnect' && (
          <span className="text-[var(--gw-amber)]">AI: {ai.providerShort} needs reconnecting</span>
        )}
        <span className="text-accent-text">tasks.md watched · saved instantly</span>
      </div>
    </div>
  )
}
