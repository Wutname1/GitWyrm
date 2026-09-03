import type { SpecChange } from '@/lib/bindings'
import { nextTask } from '@/hooks/useOpenspec'
import { useAskStore } from '@/stores/askStore'
import { useWorkspaceStore } from '@/stores/workspaceStore'
import { useStartAgentSession } from '@/hooks/useStartAgentSession'
import { openSpecTaskSourceInput } from '@/lib/agentDeskSources'

/**
 * Start work on a change's next task.
 *
 * Shared so the rail's button and Ask's escalation start the *same* work
 * rather than two near-copies that could drift on which task they pick.
 *
 * This used to call `aiRunStart`, which ran the task through a second,
 * independent lifecycle: its own registry, its own event stream, and no
 * durable session behind it. Work started that way was invisible to
 * everything Agent Desk provides -- it recorded no usage, was never checked
 * over afterwards, could not be recovered after a restart, and left no
 * result anyone could keep or undo. Two lifecycles for one job also meant
 * every fix to one of them silently missed the other.
 *
 * So there is one execution path now, and it is Agent Desk's. Spec Desk
 * keeps its editor and its browsing; what it no longer keeps is a private
 * way to run things.
 *
 * The isolated-folder choice goes away with it, and that is a real change
 * in behaviour rather than an oversight: an Agent Desk session on a Fix
 * intent always provisions its own worktree, so the thing the checkbox
 * asked for is now what always happens.
 */
export function useStartRun(repoId: string, change: SpecChange) {
  const { startSession, starting } = useStartAgentSession()
  const clearAsk = useAskStore((s) => s.clear)
  const repo = useWorkspaceStore((s) => s.openRepos.find((r) => r.id === repoId) ?? null)
  const task = nextTask(change)

  const startRun = async () => {
    if (!task || starting || !repo) return
    // Any ask session ends here, and its epoch bump drops replies still in
    // flight. Without this an answer could land in the tab after the work
    // took it over, which is exactly the mode confusion this avoids.
    clearAsk(repoId)
    await startSession({
      key: `openspec-task:${change.id}:${task.index}`,
      repoId,
      repoPath: repo.path,
      repoName: repo.name,
      // The task the user is actually looking at, not a recomputed one.
      source: openSpecTaskSourceInput(change, task),
      intent: 'fix',
    })
  }

  return { startRun, starting, canStart: task != null && repo != null }
}
