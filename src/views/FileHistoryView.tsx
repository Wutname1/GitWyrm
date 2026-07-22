import { CornerDownRight } from 'lucide-react'
import { cn } from '@/lib/utils'
import { authorColor, formatCommitTime, shortSha } from '@/lib/gitDisplay'
import { useFileHistory } from '@/hooks/useGitQueries'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo } from '@/stores/workspaceStore'
import { Avatar } from '@/components/domain/graph/Avatar'
import { AuthorHoverCard } from '@/components/domain/graph/AuthorHoverCard'
import { FileViewHeader } from '@/components/domain/file/FileViewHeader'

function initials(name: string): string {
  const words = name.split(/\s+/).filter(Boolean)
  if (words.length >= 2) return (words[0][0] + words[words.length - 1][0]).toUpperCase()
  return name.slice(0, 2).toUpperCase()
}

/**
 * Every commit that touched one file, newest first. Clicking a row opens that
 * commit's version of the file in the diff view, so the list doubles as a way
 * to step back through the file's changes.
 */
export function FileHistoryView() {
  const repo = useActiveRepo()
  const target = useUiStore((s) => s.fileTarget)
  const openDiff = useUiStore((s) => s.openDiff)
  const selectCommit = useUiStore((s) => s.selectCommit)
  const diffRequest = useUiStore((s) => s.diffRequest)
  const history = useFileHistory(repo?.id ?? null, target?.path ?? null)

  if (!target) return null

  const entries = history.data?.entries ?? []

  return (
    <>
      <FileViewHeader path={target.path} mode="history" />

      <div className="min-h-0 flex-1 overflow-y-auto">
        {history.isLoading && (
          <div className="p-4 text-center text-xs text-muted-foreground">Reading history…</div>
        )}
        {history.isError && (
          <div className="p-4 text-center text-xs text-removed">
            {(history.error as Error).message}
          </div>
        )}
        {history.data && entries.length === 0 && (
          <div className="p-4 text-center text-xs text-muted-foreground">
            No commits have changed this file yet.
          </div>
        )}

        {entries.map((e) => {
          const open =
            diffRequest?.source.kind === 'commit' &&
            diffRequest.source.sha === e.sha &&
            diffRequest.path === target.path
          return (
            <div
              key={e.sha}
              onClick={() => {
                selectCommit(e.sha)
                openDiff({ path: target.path, source: { kind: 'commit', sha: e.sha } })
              }}
              className={cn(
                'flex cursor-pointer items-center gap-2.5 border-b border-border/50 px-3.5 py-2 hover:bg-panel2',
                open && 'bg-soft hover:bg-soft'
              )}
            >
              <AuthorHoverCard
                name={e.author_name}
                email={e.author_email}
                initials={initials(e.author_name)}
              >
                <span className="flex-none cursor-default">
                  <Avatar
                    initials={initials(e.author_name)}
                    color={authorColor(e.author_name)}
                    email={e.author_email}
                    size="md"
                  />
                </span>
              </AuthorHoverCard>

              <div className="min-w-0 flex-1">
                <div className="overflow-hidden text-ellipsis whitespace-nowrap text-xs text-foreground">
                  {e.summary}
                </div>
                <div className="flex items-center gap-1.5 text-2xs text-muted-foreground">
                  <span>{e.author_name}</span>
                  <span>·</span>
                  <span>{formatCommitTime(e.time)}</span>
                  {e.old_path && (
                    <>
                      <span>·</span>
                      <span className="flex items-center gap-1 text-sub">
                        <CornerDownRight className="size-3" />
                        renamed from{' '}
                        <span className="font-mono">{e.old_path}</span>
                      </span>
                    </>
                  )}
                </div>
              </div>

              <span className="font-mono text-2xs text-added">+{e.additions}</span>
              <span className="font-mono text-2xs text-removed">-{e.deletions}</span>
              <span className="flex-none rounded-[5px] border border-border bg-panel2 px-2 py-[3px] font-mono text-2xs text-sub">
                {shortSha(e.sha)}
              </span>
            </div>
          )
        })}

        {history.data?.has_more && (
          <div className="p-3 text-center text-2xs text-muted-foreground">
            Showing the most recent changes to this file.
          </div>
        )}
      </div>
    </>
  )
}
