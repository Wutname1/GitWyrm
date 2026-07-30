import { useEffect, useState } from 'react'
import { commands, type RepoInfo } from '@/lib/bindings'
import { unwrap } from '@/lib/queryKeys'
import { DeskTitleBar } from '@/components/domain/spec-desk/DeskTitleBar'
import { DeskChangesList } from '@/components/domain/spec-desk/DeskChangesList'
import { DeskDetail } from '@/components/domain/spec-desk/DeskDetail'
import { useOpenspecChanges, useOpenspecStatus, useSelectedChange } from '@/hooks/useOpenspec'
import { listenForSpecSelection } from '@/lib/specSync'
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

  // Selections made in the main window have to move this window too.
  useEffect(() => listenForSpecSelection(), [])

  const repoName = repo?.name ?? 'Loading…'

  return (
    <div className="flex h-screen w-screen flex-col overflow-hidden bg-bg text-foreground">
      <DeskTitleBar repoName={repoName} />

      {error ? (
        <DeskMessage title="This Spec Desk has nothing to show" detail={error} />
      ) : !repo ? (
        <DeskMessage title="Opening the repository…" detail="This takes a moment on first open." />
      ) : !status.data?.present ? (
        <DeskMessage
          title="This repository does not use OpenSpec"
          detail="Add an openspec/ folder to plan work here. Everything else in GitWyrm works as usual."
        />
      ) : (
        <div className="grid min-h-0 flex-1 grid-cols-[282px_minmax(0,1fr)]">
          <DeskChangesList
            changes={changes}
            archivedCount={status.data.archived_count}
            selectedId={change?.id}
          />

          {change ? (
            <DeskDetail change={change} repoId={repo.id} />
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
        <span className="text-accent-text">tasks.md watched · saved instantly</span>
      </div>
    </div>
  )
}
