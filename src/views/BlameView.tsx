import { useMemo } from 'react'
import { cn } from '@/lib/utils'
import { commitColor, formatCommitTime, shortSha } from '@/lib/gitDisplay'
import { useFileBlame } from '@/hooks/useGitQueries'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo } from '@/stores/workspaceStore'
import { AuthorHoverCard } from '@/components/domain/graph/AuthorHoverCard'
import { FileViewHeader } from '@/components/domain/file/FileViewHeader'
import type { BlameLine } from '@/lib/bindings'

function initials(name: string): string {
  const words = name.split(/\s+/).filter(Boolean)
  if (words.length >= 2) return (words[0][0] + words[words.length - 1][0]).toUpperCase()
  return name.slice(0, 2).toUpperCase()
}

/**
 * A run of consecutive lines that share one commit. Blame is far easier to read
 * when a ten-line block from one commit says so once, rather than repeating the
 * same author and sha on every line.
 */
interface Block {
  sha: string
  lines: BlameLine[]
}

function toBlocks(lines: BlameLine[]): Block[] {
  const blocks: Block[] = []
  for (const line of lines) {
    const last = blocks[blocks.length - 1]
    if (last && last.sha === line.sha) last.lines.push(line)
    else blocks.push({ sha: line.sha, lines: [line] })
  }
  return blocks
}

export function BlameView() {
  const repo = useActiveRepo()
  const target = useUiStore((s) => s.fileTarget)
  const selectCommit = useUiStore((s) => s.selectCommit)
  const openDiff = useUiStore((s) => s.openDiff)
  const blame = useFileBlame(repo?.id ?? null, target?.path ?? null, target?.sha ?? null)

  const blocks = useMemo(() => toBlocks(blame.data?.lines ?? []), [blame.data])

  if (!target) return null

  return (
    <>
      <FileViewHeader
        path={target.path}
        mode="blame"
        pinnedLabel={target.sha ? `at ${shortSha(target.sha)}` : null}
      />

      <div className="min-h-0 flex-1 overflow-auto">
        {blame.isLoading && (
          <div className="p-4 text-center text-xs text-muted-foreground">
            Loading blame…
          </div>
        )}
        {blame.isError && (
          <div className="p-4 text-center text-xs text-removed">
            {(blame.error as Error).message}
          </div>
        )}
        {blame.data?.binary && (
          <div className="p-4 text-center text-xs text-muted-foreground">
            This is a binary file, so there are no lines to trace.
          </div>
        )}
        {blame.data && !blame.data.binary && blocks.length === 0 && (
          <div className="p-4 text-center text-xs text-muted-foreground">This file is empty.</div>
        )}

        {blocks.map((block) => {
          const first = block.lines[0]
          const color = commitColor(block.sha)
          return (
            <div key={`${block.sha}:${first.line_no}`} className="flex items-stretch">
              {/* Who and when, once per run of lines from the same commit. */}
              <div
                onClick={() => {
                  selectCommit(block.sha)
                  openDiff({ path: target.path, source: { kind: 'commit', sha: block.sha } })
                }}
                title={first.summary}
                style={{ borderLeftColor: color }}
                className="flex w-56 flex-none cursor-pointer flex-col justify-start gap-px border-l-2 bg-panel px-2 py-0.5 hover:bg-panel2"
              >
                <div className="flex items-center gap-1.5">
                  <span className="font-mono text-2xs text-sub">{shortSha(block.sha)}</span>
                  <AuthorHoverCard
                    name={first.author_name}
                    email={first.author_email}
                    initials={initials(first.author_name)}
                  >
                    <span className="overflow-hidden text-ellipsis whitespace-nowrap text-2xs text-muted-foreground hover:text-foreground">
                      {first.author_name}
                    </span>
                  </AuthorHoverCard>
                </div>
                <span className="whitespace-nowrap text-2xs text-muted-foreground">
                  {formatCommitTime(first.time)}
                </span>
              </div>

              <div className="min-w-0 flex-1">
                {block.lines.map((line) => (
                  <div key={line.line_no} className="flex items-start hover:bg-panel2">
                    <span className="w-12 flex-none select-none pr-2 text-right font-mono text-2xs leading-5 text-muted-foreground">
                      {line.line_no}
                    </span>
                    <pre
                      className={cn(
                        'min-w-0 flex-1 overflow-x-auto whitespace-pre px-2 font-mono text-xs leading-5 text-foreground'
                      )}
                    >
                      {line.text || ' '}
                    </pre>
                  </div>
                ))}
              </div>
            </div>
          )
        })}
      </div>
    </>
  )
}
