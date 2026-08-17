import { useState } from 'react'
import { Pencil, X } from 'lucide-react'
import { toast } from 'sonner'
import { copyToClipboard } from '@/lib/clipboard'
import { Button } from '@/components/ui/button'
import { TooltipButton, TooltipHint } from '@/components/ui/tooltip'
import { authorColor, formatCommitTime, shortSha } from '@/lib/gitDisplay'
import { useCommitDetail, useCommitEntry, useStashes } from '@/hooks/useGitQueries'
import { useCommitPr } from '@/hooks/useGithub'
import { matchExplanation, type CommitPrMatch } from '@/lib/commitPr'
import { cn } from '@/lib/utils'
import { GithubItemIcon } from '@/lib/githubDisplay'
import { useGitMutations } from '@/hooks/useGitMutations'
import { useUiStore } from '@/stores/uiStore'
import { RewordDialog } from '@/components/modals/RewordDialog'
import { Avatar } from './Avatar'
import { AuthorHoverCard } from './AuthorHoverCard'
import { FileChangeRow } from '../FileChangeRow'

function initials(name: string): string {
  const words = name.split(/\s+/).filter(Boolean)
  if (words.length >= 2) return (words[0][0] + words[words.length - 1][0]).toUpperCase()
  return name.slice(0, 2).toUpperCase()
}

/**
 * The pull request a commit belongs to, shown beside its message.
 *
 * Clicking opens the pull request in the center view. A pull request that has
 * already been merged is no longer in the open list, so there is nothing to
 * open -- the chip still names the number, because that is a true and useful
 * thing to say about the commit, but it renders as plain text rather than a
 * button that would go nowhere.
 */
function PrChip({ match, onOpen }: { match: CommitPrMatch; onOpen: () => void }) {
  const label = `#${match.number}${match.draft ? ' · draft' : ''}`
  const body = (
    <>
      <GithubItemIcon kind="pr" size={11} />
      <span className="font-mono">{label}</span>
      <span className="max-w-[14ch] overflow-hidden text-ellipsis whitespace-nowrap">
        {match.title}
      </span>
    </>
  )
  const className =
    'flex flex-none items-center gap-1 rounded-[5px] border border-primary/30 bg-soft px-2 py-[3px] text-2xs text-accent-text'

  if (match.webUrl === '') {
    return (
      <TooltipHint label={`${matchExplanation(match)} It is no longer open, so there is nothing to show.`}>
        <span className={cn(className, 'cursor-default opacity-80')}>{body}</span>
      </TooltipHint>
    )
  }
  return (
    <TooltipHint label={`${matchExplanation(match)} Click to open it.`}>
      <button
        type="button"
        onClick={onOpen}
        className={cn(className, 'hover:border-primary/60 hover:bg-panel3')}
      >
        {body}
      </button>
    </TooltipHint>
  )
}

export function CommitDrawer({ repoId, sha }: { repoId: string; sha: string }) {
  const openGithubItem = useUiStore((s) => s.openGithubItem)
  const selectCommit = useUiStore((s) => s.selectCommit)
  const revealShaInGraph = useUiStore((s) => s.revealShaInGraph)
  const openDiff = useUiStore((s) => s.openDiff)
  const diffRequest = useUiStore((s) => s.diffRequest)
  const detail = useCommitDetail(repoId, sha)
  const m = useGitMutations(repoId)
  const [rewordOpen, setRewordOpen] = useState(false)

  // A stash is a commit, so it opens in this drawer like any other -- but it
  // has several parents, which the reword path can't rebuild. Renaming one goes
  // through the stash's own right-click menu instead, so hide the pencil here
  // rather than offer a button that always fails.
  const stashes = useStashes(repoId)
  const isStash = stashes.data?.some((s) => s.sha === sha) ?? false

  // The graph row carries the refs a `CommitDetail` does not, so the branch-tip
  // route works here too. Falls back to the detail alone when the commit is not
  // on a loaded log page.
  const graphCommit = useCommitEntry(repoId, sha)
  const matchable = detail.data
    ? {
        sha: detail.data.sha,
        summary: detail.data.summary,
        refs: graphCommit?.refs ?? [],
      }
    : null
  const pr = useCommitPr(repoId, matchable)

  // Highlight the row whose diff is on screen, but only when that diff came
  // from this commit -- otherwise a same-named file from the pending changes
  // list would light up a row it has nothing to do with.
  const openPath =
    diffRequest?.source.kind === 'commit' && diffRequest.source.sha === sha
      ? diffRequest.path
      : null

  if (detail.isLoading) {
    return (
      <div className="flex h-full flex-none items-center justify-center border-t border-border bg-panel text-xs text-muted-foreground">
        Loading commit…
      </div>
    )
  }
  if (detail.isError || !detail.data) {
    return (
      <div className="flex h-full flex-none items-center justify-center border-t border-border bg-panel text-xs text-removed">
        {(detail.error as Error | null)?.message ?? 'Failed to load commit'}
      </div>
    )
  }

  const d = detail.data
  const color = authorColor(d.author_name)
  const adds = d.files.reduce((a, f) => a + f.additions, 0)
  const dels = d.files.reduce((a, f) => a + f.deletions, 0)

  return (
    <div className="flex h-full min-h-0 flex-none flex-col border-t border-border bg-panel">
      <div className="flex max-h-[45%] flex-none items-start gap-2.5 overflow-y-auto border-b border-border px-3.5 py-[9px]">
        <AuthorHoverCard name={d.author_name} email={d.author_email} initials={initials(d.author_name)}>
          <span className="flex-none cursor-default">
            <Avatar
              initials={initials(d.author_name)}
              color={color}
              email={d.author_email}
              size="md"
            />
          </span>
        </AuthorHoverCard>
        <div className="min-w-0 flex-1">
          <div className="flex min-w-0 items-start gap-2">
            <div className="min-w-0 flex-1 select-text break-words text-[0.78125rem] font-semibold text-foreground">
              {d.summary}
            </div>
            {pr && <PrChip match={pr} onOpen={() => openGithubItem('pr', pr.number)} />}
          </div>
          <div className="select-text text-2xs text-muted-foreground">
            <AuthorHoverCard
              name={d.author_name}
              email={d.author_email}
              initials={initials(d.author_name)}
            >
              <span className="cursor-default hover:text-foreground">{d.author_name}</span>
            </AuthorHoverCard>{' '}
            committed · {formatCommitTime(d.time)} ·{' '}
            {d.parent_shas.length === 0 ? (
              <>no parent (first commit)</>
            ) : (
              <>
                {d.parent_shas.length > 1 ? 'parents' : 'parent'}{' '}
                {d.parent_shas.map((p, i) => (
                  <span key={p}>
                    {i > 0 && ', '}
                    <TooltipHint label={`Go to parent commit ${shortSha(p)}`}>
                      <button
                        type="button"
                        onClick={() => revealShaInGraph(p)}
                        className="rounded-sm font-mono text-accent-text underline-offset-2 hover:underline focus-visible:outline focus-visible:outline-1 focus-visible:outline-offset-1 focus-visible:outline-[var(--gw-accent)]"
                      >
                        {shortSha(p)}
                      </button>
                    </TooltipHint>
                  </span>
                ))}
              </>
            )}
          </div>
        </div>
        <span className="mt-0.5 flex-none select-text rounded-[5px] border border-border bg-panel2 px-2 py-[3px] font-mono text-2xs text-sub">
          {shortSha(d.sha)}
        </span>
        <Button
          variant="secondary"
          size="sm"
          className="mt-0.5 h-auto flex-none rounded border-border bg-panel3 px-[7px] py-0.5 text-2xs text-sub"
          onClick={() => void copyToClipboard(d.sha, `Copied ${shortSha(d.sha)}`)}
        >
          Copy SHA
        </Button>
        <Button
          variant="secondary"
          size="sm"
          className="mt-0.5 h-auto flex-none rounded border-border bg-panel3 px-[7px] py-0.5 text-2xs text-sub"
          onClick={() =>
            void copyToClipboard(
              d.body.trim() === '' ? d.summary : `${d.summary}\n\n${d.body}`,
              'Copied commit message'
            )
          }
        >
          Copy message
        </Button>
        {!isStash && (
          <TooltipButton
            onClick={() => setRewordOpen(true)}
            tooltip="Edit this message"
            className="mt-0.5 flex size-6 flex-none items-center justify-center rounded-[5px] border border-border bg-panel2 text-xs text-sub hover:border-muted-foreground hover:bg-panel3"
          >
            <Pencil size={12} />
          </TooltipButton>
        )}
        <TooltipButton
          onClick={() => selectCommit(null)}
          tooltip="Close"
          className="mt-0.5 flex size-6 flex-none items-center justify-center rounded-[5px] border border-border bg-panel2 text-xs text-sub hover:border-muted-foreground hover:bg-panel3"
        >
          <X size={12} />
        </TooltipButton>
      </div>
      {d.body.trim() !== '' && (
        <div className="max-h-[30%] flex-none select-text overflow-y-auto whitespace-pre-wrap break-words border-b border-border px-3.5 py-2 text-2xs leading-relaxed text-sub">
          {d.body}
        </div>
      )}
      <div className="flex flex-none items-center gap-3.5 border-b border-border px-3.5 py-[5px] text-2xs text-sub">
        <span className="font-semibold">{d.files.length} files changed</span>
        <span className="font-mono text-added">+{adds}</span>
        <span className="font-mono text-removed">-{dels}</span>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto py-1">
        {d.files.map((f) => (
          <FileChangeRow
            key={f.path}
            file={f}
            mono
            nameClassName="text-sub"
            menuSha={d.sha}
            active={openPath === f.path}
            onOpen={() => openDiff({ path: f.path, source: { kind: 'commit', sha: d.sha } })}
          />
        ))}
      </div>
      <RewordDialog
        open={rewordOpen}
        onOpenChange={setRewordOpen}
        initialSummary={d.summary}
        initialBody={d.body}
        pending={m.rewordCommit.isPending}
        onConfirm={(message) =>
          m.rewordCommit.mutate(
            { sha: d.sha, message },
            {
              onSuccess: (newSha) => {
                setRewordOpen(false)
                // The old sha no longer exists after a reword; follow the commit
                // to its new sha so the drawer keeps showing it.
                if (newSha !== d.sha) selectCommit(newSha)
              },
            }
          )
        }
      />
    </div>
  )
}
