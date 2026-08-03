import { useState } from 'react'
import { toast } from 'sonner'
import type { SpecChange } from '@/lib/bindings'
import { commands } from '@/lib/bindings'
import { nextTask } from '@/hooks/useOpenspec'
import { useBranches } from '@/hooks/useGitQueries'
import { useAskStore } from '@/stores/askStore'

/**
 * Start a monitored run on a change's next task.
 *
 * Shared so the rail's button and Ask's escalation start the *same* run rather
 * than two near-copies that could drift on which task or branch they pick. Ask
 * offering a run it starts differently from the rail would be the kind of
 * inconsistency nobody notices until it matters.
 */
export function useStartRun(repoId: string, change: SpecChange) {
  const [starting, setStarting] = useState(false)
  const branches = useBranches(repoId)
  const clearAsk = useAskStore((s) => s.clear)
  const task = nextTask(change)

  /**
   * `ownFolder` runs the task in its own copy of the project so the user can
   * keep editing theirs while it works.
   *
   * Off by default: a run already promises "your own work is untouched" and
   * already delivers it by setting uncommitted changes aside before starting.
   * Making every run create a folder on disk would trade a promise the user
   * already has for one they must now clean up. Isolation earns its cost in one
   * case -- wanting to keep working in the same files while a run goes -- so
   * that is where it is offered.
   */
  const startRun = async (ownFolder = false) => {
    if (!task || starting) return
    setStarting(true)
    // Any ask session ends here, and its epoch bump drops replies still in
    // flight. Without this an answer could land in the ✦ tab after the run took
    // it over, which is exactly the mode confusion this feature exists to avoid.
    clearAsk(repoId)
    try {
      const res = await commands.aiRunStart(
        repoId,
        change.id,
        task.index,
        // The author's own numbering is what the header shows; the index is
        // what identifies the checkbox. They disagree whenever a plan skips or
        // repeats a number, so both are sent rather than one being derived.
        change.progress.done + 1,
        task.text,
        // The checked-out branch: a run edits files in place, so it has to be
        // pinned to where the work will actually land.
        branches.data?.local.find((b) => b.is_head)?.name ?? 'main',
        ownFolder
      )
      if (res.status !== 'ok') {
        // A folder that could not be made fails the start outright rather than
        // quietly working in the user's checkout instead -- they asked to keep
        // editing, and silently taking that away is the worse outcome.
        toast.error('That could not be started.', { description: res.error })
        return
      }
      if (res.data.kind === 'alreadyRunning') {
        toast.info(res.data.summary)
        return
      }
      toast.success('Started. Watch it on the AI tab.', {
        description: ownFolder
          ? 'It has its own copy of your files, so you can keep working in yours.'
          : undefined,
      })
    } finally {
      setStarting(false)
    }
  }

  return { startRun, starting, canStart: task != null }
}
