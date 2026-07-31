import { useEffect, useState } from 'react'
import { GitCommitHorizontal, Undo2 } from 'lucide-react'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { Textarea } from '@/components/ui/textarea'
import { commands } from '@/lib/bindings'
import { unwrap } from '@/lib/queryKeys'
import { describeError } from '@/lib/log'

/**
 * What a finished run offers: the work it did, and a commit to approve.
 *
 * The commit click stays the user's. The run proves itself, drafts a message,
 * and stops -- an agent that committed on its own would be a different and much
 * harder thing to trust inside a git client. The message is editable here
 * because a draft the user cannot correct is not really being approved.
 */
export function RunFinishedCard({
  repoId,
  changeId,
  taskNumber,
  provider,
  onCommitted,
  onUndo,
}: {
  repoId: string
  changeId: string
  taskNumber: number
  /** Display name, for the Assisted-by trailer. */
  provider: string
  onCommitted: (sha: string) => void
  onUndo: () => void
}) {
  const [message, setMessage] = useState<string | null>(null)
  const [taskLine, setTaskLine] = useState<number | null>(null)
  const [committing, setCommitting] = useState(false)

  // Draft the message from the run itself rather than from whatever is selected
  // now: the session carries its own change and task for exactly this reason.
  useEffect(() => {
    let cancelled = false
    void (async () => {
      try {
        const completion = unwrap(await commands.aiRunCompletion(repoId, provider))
        if (cancelled || !completion) return
        setMessage(completion.message)
        setTaskLine(completion.task_line)
      } catch (e) {
        if (!cancelled) setMessage(`improved: task ${taskNumber}\n\nSpec: ${changeId}`)
        void e
      }
    })()
    return () => {
      cancelled = true
    }
  }, [repoId, provider, changeId, taskNumber])

  const commit = async () => {
    if (!message?.trim() || committing) return
    setCommitting(true)
    try {
      // Stage everything the run touched, then commit the approved message as
      // written. The message already carries both trailers, so no separate
      // spec_id is passed -- one source for what lands in history.
      unwrap(await commands.stageAll(repoId))
      const [summary, ...rest] = message.trim().split('\n')
      const sha = unwrap(
        await commands.createCommit(repoId, summary, rest.join('\n').trim(), false, null)
      )
      // Tick the task only once the commit exists. Ticking first would leave a
      // change claiming work that was never recorded if the commit failed.
      if (taskLine != null) {
        try {
          unwrap(await commands.openspecToggleTask(repoId, changeId, taskLine, true))
        } catch (e) {
          toast.info('Committed, but the task box did not tick.', {
            description: describeError(e),
          })
        }
      }
      onCommitted(sha)
    } catch (e) {
      toast.error('Could not commit that.', { description: describeError(e) })
    } finally {
      setCommitting(false)
    }
  }

  return (
    <div className="rounded-lg border border-primary/40 bg-soft p-3">
      <div className="text-xs font-semibold text-accent-text">Task finished</div>
      <p className="mt-1 text-2xs leading-relaxed text-sub">
        Nothing is committed yet. Read the message, change it if you want, then commit.
      </p>

      <label className="mt-2.5 block text-2xs font-semibold text-sub">Commit message</label>
      <Textarea
        value={message ?? ''}
        onChange={(e) => setMessage(e.target.value)}
        placeholder={message == null ? 'Writing the message…' : undefined}
        disabled={message == null || committing}
        rows={5}
        className="mt-1 bg-background px-2.5 py-2 font-mono text-2xs leading-relaxed"
      />
      <p className="mt-1 text-2xs text-muted-foreground">
        The Spec and Assisted-by lines link this commit to the change and mark it as AI
        work. Leave them in place.
      </p>

      <div className="mt-2.5 flex flex-wrap gap-1.5">
        <Button
          size="sm"
          className="h-7 text-xs"
          disabled={!message?.trim() || committing}
          onClick={() => void commit()}
        >
          <GitCommitHorizontal size={12} strokeWidth={2.2} />
          {committing ? 'Committing…' : 'Commit these changes'}
        </Button>
        <Button
          variant="secondary"
          size="sm"
          className="h-7 text-xs"
          disabled={committing}
          onClick={onUndo}
        >
          <Undo2 size={12} strokeWidth={2.2} />
          Undo the AI's edits
        </Button>
      </div>
    </div>
  )
}
