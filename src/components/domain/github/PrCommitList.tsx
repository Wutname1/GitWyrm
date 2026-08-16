import { GitCommitHorizontal } from 'lucide-react'
import type { PrCommit } from '@/lib/bindings'
import { PendingIndicator } from '@/components/ui/pending-indicator'
import { copyToClipboard } from '@/lib/clipboard'
import { formatRelativeTime } from '@/lib/gitDisplay'
import { TooltipButton } from '@/components/ui/tooltip'

function ago(iso: string | null): string {
  if (!iso) return ''
  const parsed = Date.parse(iso)
  return Number.isNaN(parsed) ? '' : formatRelativeTime(parsed / 1000)
}

/** The commits behind a pull request, oldest first as the host returns them. */
export function PrCommitList({
  commits,
  loading,
  error,
}: {
  commits: PrCommit[] | undefined
  loading: boolean
  error: boolean
}) {
  return (
    <section className="mt-4 overflow-hidden rounded-md border border-border bg-panel2/70">
      <div className="flex items-center gap-2 border-b border-border px-3.5 py-2.5 text-2xs font-bold text-foreground">
        <GitCommitHorizontal size={13} className="text-muted-foreground" />
        Commits
        {commits && commits.length > 0 && (
          <span className="ml-auto font-normal text-muted-foreground">
            {commits.length === 1 ? '1 commit' : `${commits.length} commits`}
          </span>
        )}
      </div>

      {loading && (
        <div className="flex items-center gap-2 px-3.5 py-3 text-xs text-muted-foreground">
          <PendingIndicator /> Loading the commits…
        </div>
      )}
      {error && !loading && (
        <p className="px-3.5 py-3 text-xs text-removed">
          Could not load the commits. Try Refresh.
        </p>
      )}
      {!loading && !error && commits?.length === 0 && (
        <p className="px-3.5 py-3 text-xs text-muted-foreground">No commits on this branch yet.</p>
      )}

      {!loading &&
        commits?.map((c) => (
          <div
            key={c.sha}
            className="flex items-center gap-2.5 border-b border-border/60 px-3.5 py-2 last:border-b-0"
          >
            <div className="min-w-0 flex-1">
              <p className="truncate text-xs text-foreground">{c.summary}</p>
              <p className="truncate text-2xs text-muted-foreground">
                {c.author}
                {c.authored_at ? ` · ${ago(c.authored_at)}` : ''}
              </p>
            </div>
            <TooltipButton
              onClick={() => void copyToClipboard(c.sha, 'Commit ID copied')}
              tooltip="Copy this commit's ID"
              className="flex-none rounded border border-border bg-panel3 px-1.5 py-0.5 font-mono text-2xs text-sub hover:border-muted-foreground hover:text-foreground"
            >
              {c.sha.slice(0, 7)}
            </TooltipButton>
          </div>
        ))}
    </section>
  )
}
