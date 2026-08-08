import { useQuery } from '@tanstack/react-query'
import { useMemo } from 'react'
import { commands } from '@/lib/bindings'
import { unwrap } from '@/lib/queryKeys'
import { cn } from '@/lib/utils'
import { buildRows, discardedCount, type Fate, type PreviewMode, type Row } from '@/lib/syncPreview'

export type { PreviewMode }

interface SyncTreePreviewProps {
  repoId: string | null
  /** Our side of the divergence -- the local branch. */
  ours: string
  /** Their side -- usually the upstream remote branch. */
  theirs: string
  /** The option being previewed; null shows the current state with no outcome. */
  mode: PreviewMode | null
}

const MAX_PER_SIDE = 6

/**
 * A miniature commit ladder showing what an option would do to history.
 *
 * The point is that the destructive options look destructive BEFORE they run:
 * "Replace cloud" strikes through every commit it would throw away, in red.
 * Rebase shows your commits lifted above theirs, tagged as re-created. Blend
 * adds the merge commit that would appear. Nobody should have to know what
 * `--force` means to see that four commits are about to vanish.
 *
 * Newest is drawn at the top, matching the graph.
 */
export function SyncTreePreview({ repoId, ours, theirs, mode }: SyncTreePreviewProps) {
  const q = useQuery({
    queryKey: ['divergentCommits', repoId, ours, theirs],
    enabled: !!repoId && !!ours && !!theirs,
    queryFn: async () =>
      unwrap(await commands.divergentCommits(repoId!, ours, theirs, MAX_PER_SIDE)),
  })

  const rows = useMemo<Row[]>(
    () => (q.data ? buildRows(q.data.ours, q.data.theirs, mode, ours, theirs) : []),
    [q.data, mode, ours, theirs]
  )

  if (q.isLoading) {
    return (
      <div className="rounded-md border border-border bg-panel2 px-3 py-3 text-2xs text-muted-foreground">
        Working out what changes…
      </div>
    )
  }
  if (q.isError || rows.length === 0) return null

  const discarded = discardedCount(rows)

  return (
    <div className="rounded-md border border-border bg-panel2 px-3 py-2.5">
      <div className="mb-2 flex items-center justify-between">
        <span className="text-2xs uppercase tracking-[.06em] text-muted-foreground">
          {mode ? 'What you’ll end up with' : 'How things stand now'}
        </span>
        {discarded > 0 && (
          <span className="text-2xs font-semibold text-removed">
            {discarded} change{discarded === 1 ? '' : 's'} lost
          </span>
        )}
      </div>

      <ul className="flex flex-col gap-[3px]">
        {rows.map((row) => (
          <li key={row.key} className="flex items-center gap-2">
            <Dot side={row.side} fate={row.fate} />
            <span
              className={cn(
                'min-w-0 flex-1 overflow-hidden text-ellipsis whitespace-nowrap text-2xs',
                row.fate === 'discard' && 'text-removed line-through opacity-70',
                row.fate === 'replay' && 'text-accent-text',
                row.fate === 'merge' && 'text-modified italic',
                row.fate === 'keep' && 'text-foreground'
              )}
            >
              {row.summary}
            </span>
            {row.sha && (
              <span
                className={cn(
                  'flex-none font-mono text-2xs text-muted-foreground',
                  row.fate === 'discard' && 'line-through'
                )}
              >
                {row.sha}
              </span>
            )}
            {row.fate === 'replay' && (
              <span className="flex-none text-2xs text-muted-foreground">rebuilt</span>
            )}
          </li>
        ))}
      </ul>

      {q.data?.truncated && (
        <div className="mt-1.5 text-2xs text-muted-foreground">…and older changes.</div>
      )}
      <Legend mode={mode} ours={ours} theirs={theirs} />
    </div>
  )
}

/** The commit dot, colored by whose commit it is and what happens to it. */
function Dot({ side, fate }: { side: Row['side']; fate: Fate }) {
  return (
    <span
      className={cn(
        'size-2 flex-none rounded-full border',
        fate === 'discard' && 'border-removed bg-transparent',
        fate === 'replay' && 'border-primary bg-soft',
        fate === 'merge' && 'border-modified bg-transparent',
        fate === 'keep' && side === 'ours' && 'border-primary bg-primary',
        fate === 'keep' && side === 'theirs' && 'border-border bg-panel'
      )}
    />
  )
}

function Legend({
  mode,
  ours,
  theirs,
}: {
  mode: PreviewMode | null
  ours: string
  theirs: string
}) {
  if (!mode) return null
  const text: Record<PreviewMode, string> = {
    replace: `${theirs} is overwritten with ${ours}. The struck-out changes are gone for good.`,
    reset: `${ours} is thrown away and made to match ${theirs} exactly. The struck-out changes are gone for good.`,
    stack: `Your changes are rebuilt on top of ${theirs}, so history stays a straight line.`,
    blend: `Both sets of changes are kept, joined by one new blend commit.`,
    'catch-up': `${ours} simply moves up to ${theirs}. Nothing of yours is affected.`,
  }
  return <div className="mt-2 border-t border-border pt-1.5 text-2xs text-sub">{text[mode]}</div>
}
