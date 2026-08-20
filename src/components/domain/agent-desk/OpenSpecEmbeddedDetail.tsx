import { DeskDetail } from '@/components/domain/spec-desk/DeskDetail'
import { DeskActionRail } from '@/components/domain/spec-desk/DeskActionRail'
import { DeskChangesList } from '@/components/domain/spec-desk/DeskChangesList'
import { useOpenspecChanges, useOpenspecStatus, useSelectedChange } from '@/hooks/useOpenspec'

/**
 * Keeps the existing OpenSpec change detail, tabs, and handoff actions
 * reachable inside Agent Desk (task 2.3), by embedding the Spec Desk
 * components unchanged rather than rebuilding them against `AgentSession`.
 *
 * `DeskDetail`/`DeskActionRail` operate on `SpecChange`, a model disjoint
 * from `AgentSession` -- an OpenSpec change has tasks, deltas, and a
 * proposal; an agent session has messages and an execution graph. Porting
 * their ~2000 combined lines onto the session model would be a rewrite, not
 * a reuse, so this component is a read-only embedding point: it supplies the
 * repo id and the currently selected change (via the existing
 * `useSelectedChange`/`useUiStore` global, exactly as `SpecDeskView` does)
 * and renders the same three components Spec Desk always has.
 *
 * This is intentionally NOT a `ConversationPane` -- OpenSpec changes are not
 * `AgentSession` sources yet. It is the "source/detail view" `design.md`'s
 * Migration section calls for: existing OpenSpec behavior stays reachable
 * while the rest of the shell moves to the session model around it.
 */
export function OpenSpecEmbeddedDetail({ repoId, repoPath }: { repoId: string; repoPath: string }) {
  const status = useOpenspecStatus(repoId)
  const changesQuery = useOpenspecChanges(repoId)
  const { change } = useSelectedChange(repoId)
  const changes = changesQuery.data ?? []

  return (
    <div className="grid min-h-0 flex-1 grid-cols-[220px_minmax(0,1fr)_306px]">
      <DeskChangesList
        changes={changes}
        archivedCount={status.data?.archived_count ?? 0}
        selectedId={change?.id}
        repoId={repoId}
      />
      {change ? (
        <>
          <DeskDetail change={change} repoId={repoId} />
          <DeskActionRail change={change} repoId={repoId} repoPath={repoPath} />
        </>
      ) : (
        <div className="col-span-2 flex items-center justify-center p-8">
          <p className="max-w-xs text-center text-xs text-muted-foreground">
            Select a change on the left to see its tasks and handoff actions.
          </p>
        </div>
      )}
    </div>
  )
}
