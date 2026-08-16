import { Download, GitCommitHorizontal } from 'lucide-react'
import type { PrCommit } from '@/lib/bindings'
import { PendingIndicator } from '@/components/ui/pending-indicator'
import { Button } from '@/components/ui/button'
import { copyToClipboard } from '@/lib/clipboard'
import { formatRelativeTime } from '@/lib/gitDisplay'
import { TooltipButton, TooltipHint } from '@/components/ui/tooltip'
import { useGitMutations } from '@/hooks/useGitMutations'
import { useLocalCommits } from '@/hooks/useGithub'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo } from '@/stores/workspaceStore'
import { cn } from '@/lib/utils'

function ago(iso: string | null): string {
  if (!iso) return ''
  const parsed = Date.parse(iso)
  return Number.isNaN(parsed) ? '' : formatRelativeTime(parsed / 1000)
}

/**
 * The commits behind a pull request, oldest first as the host returns them.
 *
 * Clicking one selects it in the commit graph, which is where a commit's changed
 * files already live -- rather than building a second commit viewer here.
 *
 * A commit can only be opened if this clone has the object. The host names the
 * sha either way, so a pull request nobody has fetched (or one from a fork)
 * lists commits whose contents are genuinely not here yet; those rows say so and
 * offer a fetch instead of failing on click.
 */
export function PrCommitList({
  commits,
  loading,
  error,
}: {
  commits: PrCommit[] | undefined
  loading: boolean
  error: boolean
}) {
  const repo = useActiveRepo()
  const selectCommit = useUiStore((s) => s.selectCommit)
  const closeGithubItem = useUiStore((s) => s.closeGithubItem)
  const git = useGitMutations(repo?.id ?? null)

  const shas = commits?.map((c) => c.sha) ?? []
  const local = useLocalCommits(repo?.id ?? null, shas)

  const openCommit = (sha: string) => {
    selectCommit(sha)
    // Back to the graph, where the selected commit's files are shown.
    // closeGithubItem returns the center view to the graph itself.
    closeGithubItem()
  }

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
        commits?.map((c) => {
          // Until the check answers, assume the commit is openable: the common
          // case is a fetched branch, and a row that starts disabled and turns
          // clickable reads as broken.
          const isLocal = !local.data || local.data.has(c.sha)
          return (
            <div
              key={c.sha}
              className="flex items-center gap-2.5 border-b border-border/60 px-3.5 py-2 last:border-b-0"
            >
              {isLocal ? (
                <button
                  type="button"
                  onClick={() => openCommit(c.sha)}
                  className="min-w-0 flex-1 rounded text-left hover:text-accent-text"
                  title="See what this commit changed"
                >
                  <p className="truncate text-xs text-foreground hover:text-accent-text">
                    {c.summary}
                  </p>
                  <p className="truncate text-2xs text-muted-foreground">
                    {c.author}
                    {c.authored_at ? ` · ${ago(c.authored_at)}` : ''}
                  </p>
                </button>
              ) : (
                <TooltipHint label="This commit is not downloaded yet. Fetch to see what it changed.">
                  <div className="min-w-0 flex-1">
                    <p className="truncate text-xs text-sub">{c.summary}</p>
                    <p className="truncate text-2xs text-muted-foreground">
                      {c.author}
                      {c.authored_at ? ` · ${ago(c.authored_at)}` : ''}
                    </p>
                  </div>
                </TooltipHint>
              )}

              {!isLocal && (
                <Button
                  variant="secondary"
                  size="sm"
                  className="h-6 flex-none px-1.5 text-2xs"
                  disabled={git.fetch.isPending}
                  aria-busy={git.fetch.isPending || undefined}
                  onClick={() => git.fetch.mutate(undefined)}
                  tooltip="Download the commits on this pull request"
                >
                  {git.fetch.isPending ? <PendingIndicator /> : <Download size={11} />}
                  Fetch
                </Button>
              )}

              <TooltipButton
                onClick={() => void copyToClipboard(c.sha, 'Commit ID copied')}
                tooltip="Copy this commit's ID"
                className={cn(
                  'flex-none rounded border border-border bg-panel3 px-1.5 py-0.5 font-mono text-2xs',
                  'text-sub hover:border-muted-foreground hover:text-foreground'
                )}
              >
                {c.sha.slice(0, 7)}
              </TooltipButton>
            </div>
          )
        })}
    </section>
  )
}
