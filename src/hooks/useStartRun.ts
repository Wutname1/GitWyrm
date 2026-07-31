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

  const startRun = async () => {
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
        branches.data?.local.find((b) => b.is_head)?.name ?? 'main'
      )
      if (res.status !== 'ok') {
        toast.error('That could not be started.', { description: res.error })
        return
      }
      if (res.data.kind === 'alreadyRunning') {
        toast.info(res.data.summary)
        return
      }
      toast.success('Started. Watch it on the AI tab.')
    } finally {
      setStarting(false)
    }
  }

  return { startRun, starting, canStart: task != null }
}
