import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { toast } from 'sonner'
import { HardDrive, Loader2 } from 'lucide-react'
import { commands, type AgentCopyOnDisk } from '@/lib/bindings'
import { unwrap } from '@/lib/queryKeys'
import { explainCleanupOutcome, formatDiskSize, resultStateLabel, summarizeDiskUsage } from '@/lib/agentDeskResult'
import { describeError, log } from '@/lib/log'

/**
 * What Agent Desk is currently holding on this machine.
 *
 * Every agent run works in a full copy of the project so it cannot disturb
 * what the person has open. Those copies were made removable from a result
 * panel, but nothing anywhere said they existed -- so discovery happened in
 * the file manager, or when a disk filled up. For a product whose public
 * promise is a Git client with nothing to hide, an undisclosed multi-gigabyte
 * side effect is a contradiction rather than a missing feature.
 *
 * Lives in Agent Setup because this is a machine-level fact, not a per-chat
 * one, and Setup is already where machine-level concerns are answered.
 */
export function AgentCopiesOnDisk() {
  const [clearing, setClearing] = useState<string | null>(null)
  const query = useQuery({
    queryKey: ['agentCopiesOnDisk'],
    queryFn: async () => unwrap(await commands.agentResultCopiesOnDisk()),
  })

  const copies = query.data ?? []

  const clear = async (copy: AgentCopyOnDisk) => {
    if (clearing) return
    setClearing(copy.executionId)
    try {
      const outcome = unwrap(
        await commands.agentResultCleanupWorktree(copy.repoId, copy.sessionId, copy.executionId)
      )
      const { message, removed } = explainCleanupOutcome(outcome)
      if (removed) toast.success(message)
      else toast.warning(message)
      void query.refetch()
    } catch (e) {
      const detail = describeError(e)
      log.error(`agent desk: could not clear an agent copy: ${detail}`)
      toast.error('Could not clear that copy.', { description: detail })
    } finally {
      setClearing(null)
    }
  }

  if (query.isLoading) {
    return <p className="py-6 text-center text-2xs text-muted-foreground">Measuring agent copies…</p>
  }

  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-start gap-2 rounded-md border border-border bg-panel2 px-2.5 py-2">
        <HardDrive size={14} className="mt-px flex-none text-muted-foreground" aria-hidden />
        <div className="min-w-0 flex-1">
          <p className="text-xs font-medium text-foreground">{summarizeDiskUsage(copies)}</p>
          <p className="mt-0.5 text-2xs leading-relaxed text-muted-foreground">
            Each agent run works in its own copy of your project so it cannot disturb what you have open. Clearing one
            removes only that copy. Your own files are not touched.
          </p>
        </div>
      </div>

      {copies.length > 0 && (
        <ul className="flex flex-col gap-1">
          {copies.map((copy) => (
            <li
              key={copy.executionId}
              className="flex items-center gap-2 rounded-md border border-border bg-panel2 px-2.5 py-1.5"
            >
              <div className="min-w-0 flex-1">
                <p className="truncate text-2xs font-medium text-foreground" title={copy.sessionTitle}>
                  {copy.sessionTitle}
                </p>
                <p className="truncate text-2xs text-muted-foreground" title={copy.worktreePath}>
                  {formatDiskSize(copy.sizeBytes)} · {resultStateLabel(copy.state)}
                </p>
              </div>
              <button
                type="button"
                onClick={() => void clear(copy)}
                disabled={clearing !== null}
                className="flex flex-none items-center gap-1 rounded border border-border px-2 py-0.5 text-2xs font-medium text-muted-foreground hover:bg-panel3 hover:text-foreground disabled:cursor-not-allowed disabled:opacity-60"
              >
                {clearing === copy.executionId ? (
                  <Loader2 size={11} className="animate-spin motion-reduce:animate-none" aria-hidden />
                ) : null}
                {clearing === copy.executionId ? 'Clearing…' : 'Clear'}
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}
