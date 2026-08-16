import { useEffect, useState } from 'react'
import { Markdown } from '@/components/ui/markdown'
import { Copy, ExternalLink, RefreshCw, X } from 'lucide-react'
import { GithubItemIcon } from '@/lib/githubDisplay'
import { useQueryClient } from '@tanstack/react-query'
import { cn } from '@/lib/utils'
import { copyToClipboard } from '@/lib/clipboard'
import { formatRelativeTime } from '@/lib/gitDisplay'
import type { HostComment as GithubComment, IssueDetail, PrDetail } from '@/lib/bindings'
import { Button } from '@/components/ui/button'
import { PendingIndicator } from '@/components/ui/pending-indicator'
import { Textarea } from '@/components/ui/textarea'
import { TooltipButton, TooltipHint } from '@/components/ui/tooltip'
import {
  githubKeys,
  useGithubIssueDetail,
  useGithubMutations,
  useGithubPrCommits,
  useGithubPrDetail,
  useGithubPrFiles,
  useGithubSlug,
} from '@/hooks/useGithub'
import { PrCommitList } from '@/components/domain/github/PrCommitList'
import { PrFileList } from '@/components/domain/github/PrFileList'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo } from '@/stores/workspaceStore'

function ago(iso: string): string {
  const parsed = Date.parse(iso)
  return Number.isNaN(parsed) ? '' : formatRelativeTime(parsed / 1000)
}

function Avatar({ name, bot }: { name: string; bot?: boolean }) {
  return (
    <TooltipHint label={name}>
      <span
        className={cn(
          'flex size-6 flex-none items-center justify-center rounded-full text-2xs font-bold',
          bot ? 'border border-border bg-panel3 text-sub' : 'bg-panel3 text-foreground'
        )}
      >
        {name.slice(0, 2).toUpperCase()}
      </span>
    </TooltipHint>
  )
}

/** Quiet copy icon that appears on hover over the card it sits in. */
function CopyButton({
  text,
  tooltip,
  successMessage,
  className,
}: {
  text: string
  tooltip: string
  successMessage: string
  className?: string
}) {
  return (
    <TooltipButton
      onClick={() => void copyToClipboard(text, successMessage)}
      tooltip={tooltip}
      className={cn(
        'flex size-5 flex-none items-center justify-center rounded text-muted-foreground opacity-0 transition-opacity hover:bg-panel3 hover:text-foreground focus:opacity-100 group-hover/card:opacity-100',
        className
      )}
    >
      <Copy size={12} />
    </TooltipButton>
  )
}

/** One comment as markdown, with the author and time as a quoted header line. */
function commentMarkdown(c: GithubComment): string {
  return `**${c.author}** commented ${ago(c.created_at)}:\n\n${c.body.trim()}`
}

/** The whole item -- title, link, body, and every comment -- as markdown. */
function itemMarkdown(detail: IssueDetail | PrDetail, kindLabel: string): string {
  const parts = [
    `# ${detail.title}`,
    `${kindLabel} #${detail.number} - ${detail.state} - opened by ${detail.author}`,
    detail.html_url,
    '---',
    `**${detail.author}** wrote ${ago(detail.created_at)}:\n\n${detail.body.trim() || '_No description was written._'}`,
  ]
  for (const c of detail.comments) parts.push('---', commentMarkdown(c))
  return parts.join('\n\n')
}

/**
 * Issue and pull request bodies. Thin wrapper over the shared renderer so this
 * view keeps its own empty-state wording.
 */
function MarkdownBody({ text }: { text: string }) {
  return <Markdown text={text} empty="No description was written." />
}

function CommentThread({
  comments,
  replyPlaceholder,
  onReply,
  replying,
}: {
  comments: GithubComment[]
  replyPlaceholder: string
  onReply: (body: string) => void
  replying: boolean
}) {
  const [draft, setDraft] = useState('')
  return (
    <section className="mt-4 overflow-hidden rounded-md border border-border bg-panel2/70">
      <div className="flex items-center border-b border-border px-3.5 py-2.5 text-2xs font-bold text-foreground">
        Conversation
        <span className="ml-auto font-normal text-muted-foreground">
          {comments.length === 1 ? '1 message' : `${comments.length} messages`}
        </span>
      </div>
      <div className="px-3.5 pb-3.5">
        {comments.length === 0 && (
          <p className="pt-3 text-xs text-muted-foreground">No replies yet.</p>
        )}
        {comments.map((c, i) => (
          <div
            key={`${c.created_at}-${i}`}
            className="group/card grid grid-cols-[auto_minmax(0,1fr)] gap-2.5 border-b border-border/60 py-3 last:border-b-0"
          >
            <Avatar name={c.author} bot={c.author_is_bot} />
            <div className="min-w-0">
              <div className="flex items-center text-2xs">
                <span className="font-bold text-foreground">{c.author}</span>
                <span className="ml-1.5 text-muted-foreground">{ago(c.created_at)}</span>
                <CopyButton
                  className="ml-auto"
                  text={commentMarkdown(c)}
                  tooltip="Copy this comment as markdown"
                  successMessage="Comment copied as markdown"
                />
              </div>
              <div className="mt-1">
                <MarkdownBody text={c.body} />
              </div>
            </div>
          </div>
        ))}
        <div className="pt-3">
          <Textarea
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            placeholder={replyPlaceholder}
            className="min-h-[70px] bg-background text-xs"
            disabled={replying}
          />
          <div className="mt-2 flex justify-end">
            <Button
              variant="secondary"
              size="sm"
              className="h-7 text-xs"
              disabled={replying || !draft.trim()}
              onClick={() => {
                onReply(draft)
                setDraft('')
              }}
            >
              {replying ? <PendingIndicator /> : null}
              {replying ? 'Sending…' : 'Send reply'}
            </Button>
          </div>
        </div>
      </div>
    </section>
  )
}

function StatusPill({ label, tone }: { label: string; tone: 'green' | 'violet' | 'red' }) {
  return (
    <span
      className={cn(
        'ml-auto rounded-full px-2 py-0.5 text-2xs font-bold uppercase tracking-wide',
        tone === 'green' && 'bg-primary/15 text-accent-text',
        tone === 'violet' && 'bg-lane2/15 text-lane2',
        tone === 'red' && 'bg-removed/15 text-removed'
      )}
    >
      {label}
    </span>
  )
}

type PrTab = 'conversation' | 'files' | 'commits'

/**
 * Which part of a pull request is showing.
 *
 * A pull request is three separate questions -- what is being discussed, what
 * changed, and how it got there -- and stacking all three makes the discussion
 * scroll past a thousand lines of diff. Counts sit on the tabs so the sizes are
 * visible before opening either one.
 */
function PrTabs({
  tab,
  onChange,
  fileCount,
  commitCount,
  commentCount,
}: {
  tab: PrTab
  onChange: (t: PrTab) => void
  fileCount?: number
  commitCount?: number
  commentCount: number
}) {
  const tabs: { id: PrTab; label: string; count?: number }[] = [
    { id: 'conversation', label: 'Conversation', count: commentCount },
    { id: 'files', label: 'Files changed', count: fileCount },
    { id: 'commits', label: 'Commits', count: commitCount },
  ]
  return (
    <div className="mt-4 flex items-center gap-1 border-b border-border">
      {tabs.map((t) => (
        <button
          key={t.id}
          type="button"
          onClick={() => onChange(t.id)}
          className={cn(
            'relative -mb-px border-b-2 px-3 py-2 text-xs transition-colors',
            tab === t.id
              ? 'border-primary font-semibold text-foreground'
              : 'border-transparent text-muted-foreground hover:text-foreground'
          )}
        >
          {t.label}
          {t.count != null && (
            <span
              className={cn(
                'ml-1.5 rounded-full px-1.5 py-px text-2xs',
                tab === t.id ? 'bg-primary/15 text-accent-text' : 'bg-panel3 text-muted-foreground'
              )}
            >
              {t.count}
            </span>
          )}
        </button>
      ))}
    </div>
  )
}

async function openExternal(url: string) {
  const { openUrl } = await import('@tauri-apps/plugin-opener')
  await openUrl(url)
}

export function GithubView() {
  const repo = useActiveRepo()
  const item = useUiStore((s) => s.githubItem)
  const closeGithubItem = useUiStore((s) => s.closeGithubItem)
  const qc = useQueryClient()

  const slug = useGithubSlug(repo?.id ?? null)
  const pr = useGithubPrDetail(slug.data, item?.kind === 'pr' ? item.number : null, repo?.id)
  const issue = useGithubIssueDetail(
    slug.data,
    item?.kind === 'issue' ? item.number : null,
    repo?.id
  )
  const m = useGithubMutations(slug.data, repo?.id)

  const [tab, setTab] = useState<PrTab>('conversation')

  const prNumber = item?.kind === 'pr' ? item.number : null
  // Both are fetched as soon as a pull request opens rather than on tab click:
  // the counts belong on the tabs, and paying for them up front means switching
  // tabs is instant. They are separate queries from the detail, so the
  // conversation still renders while these are in flight.
  const files = useGithubPrFiles(slug.data, prNumber, repo?.id)
  const commits = useGithubPrCommits(slug.data, prNumber, repo?.id)

  // Jump back to the top when switching between items, and start each one on the
  // conversation -- the tab that was open for the last pull request says nothing
  // about this one.
  const [scrollEl, setScrollEl] = useState<HTMLDivElement | null>(null)
  useEffect(() => {
    scrollEl?.scrollTo({ top: 0 })
    setTab('conversation')
  }, [item?.kind, item?.number, scrollEl])

  if (!item) return null

  const isPr = item.kind === 'pr'
  const detail = isPr ? pr.data : issue.data
  const query = isPr ? pr : issue
  const kindLabel = isPr ? 'Pull request' : 'Issue'

  const refresh = () => {
    if (!slug.data) return
    const { owner, repo: name } = slug.data
    qc.invalidateQueries({
      queryKey: isPr
        ? githubKeys.pr(owner, name, item.number)
        : githubKeys.issue(owner, name, item.number),
    })
    // The files and commits are their own queries, so Refresh has to name them
    // too or the diffs stay stale while the conversation updates.
    if (isPr) {
      qc.invalidateQueries({ queryKey: githubKeys.prFiles(owner, name, item.number) })
      qc.invalidateQueries({ queryKey: githubKeys.prCommits(owner, name, item.number) })
    }
  }

  const statusPill = !detail ? null : isPr && pr.data ? (
    pr.data.merged ? (
      <StatusPill label="Merged" tone="violet" />
    ) : pr.data.state === 'closed' ? (
      <StatusPill label="Closed" tone="red" />
    ) : pr.data.draft ? (
      <StatusPill label="Draft" tone="violet" />
    ) : (
      <StatusPill label="Open" tone="green" />
    )
  ) : issue.data ? (
    issue.data.state === 'closed' ? (
      <StatusPill label="Closed" tone="red" />
    ) : (
      <StatusPill label="Open" tone="green" />
    )
  ) : null

  return (
    <div className="flex min-h-0 flex-1 flex-col bg-background">
      <header className="flex min-h-[52px] flex-none items-center gap-3 border-b border-border bg-panel/70 px-4 py-2">
        <span className="flex size-8 flex-none items-center justify-center rounded-md border border-primary/30 bg-soft">
          <GithubItemIcon kind={isPr ? 'pr' : 'issue'} size={15} />
        </span>
        <div className="min-w-0">
          <h2 className="truncate font-wordmark text-[0.8125rem] font-semibold text-foreground">
            {kindLabel} #{item.number}
          </h2>
          <p className="truncate text-2xs text-sub">
            {detail
              ? // Azure DevOps reports no updated time, so fall back to when it
                // was opened rather than showing a blank or a wrong "just now".
                detail.updated_at
                ? `Updated ${ago(detail.updated_at)}`
                : `Opened ${ago(detail.created_at)}`
              : query.isError
                ? 'Could not load'
                : 'Loading…'}
          </p>
        </div>
        <div className="ml-auto flex items-center gap-1.5">
          <Button
            variant="ghost"
            size="sm"
            className="h-7 px-2 text-xs"
            onClick={refresh}
            disabled={query.isFetching}
            tooltip="Check GitHub for updates"
          >
            <RefreshCw size={13} className={query.isFetching ? 'animate-spin' : undefined} />
            Refresh
          </Button>
          {detail && (
            <Button
              variant="ghost"
              size="sm"
              className="h-7 px-2 text-xs"
              onClick={() =>
                void copyToClipboard(
                  itemMarkdown(detail, kindLabel),
                  `${kindLabel} copied as markdown`
                )
              }
              tooltip="Copy the title, description, and every comment as markdown"
            >
              <Copy size={13} />
              Copy all
            </Button>
          )}
          {detail && (
            <Button
              variant="ghost"
              size="sm"
              className="h-7 px-2 text-xs"
              onClick={() => void openExternal(detail.html_url)}
              tooltip="Open this item in your browser"
            >
              <ExternalLink size={13} />
              Open on GitHub
            </Button>
          )}
          <Button
            variant="ghost"
            size="sm"
            className="h-7 px-2 text-xs"
            onClick={closeGithubItem}
            tooltip="Back to the commit graph"
          >
            <X size={13} />
          </Button>
        </div>
      </header>

      <div ref={setScrollEl} className="min-h-0 flex-1 overflow-y-auto">
        <article className="mx-auto w-full max-w-[780px] px-5 pb-20 pt-6">
          {query.isError && (
            <div className="rounded-md border border-removed/40 bg-removed/10 p-3 text-xs text-removed">
              Could not load this {kindLabel.toLowerCase()} from GitHub. Check your connection and
              try Refresh.
            </div>
          )}
          {!detail && !query.isError && (
            <div className="flex items-center gap-2 pt-8 text-xs text-muted-foreground">
              <PendingIndicator /> Loading from GitHub…
            </div>
          )}
          {detail && (
            <>
              <div className="flex items-center gap-2 text-2xs text-muted-foreground">
                <GithubItemIcon kind={isPr ? 'pr' : 'issue'} size={13} />
                <span>{kindLabel}</span>
                <span className="ml-auto font-mono">#{detail.number}</span>
              </div>
              <h3 className="mb-2.5 mt-3 select-text font-wordmark text-[1.375rem] font-semibold leading-tight tracking-tight text-foreground">
                {detail.title}
              </h3>
              <div className="flex items-center gap-2 border-b border-border pb-4 text-2xs text-sub">
                <Avatar name={detail.author} bot={isPr ? pr.data?.author_is_bot : false} />
                <span>
                  {detail.author} opened this
                  {detail.updated_at ? ` · updated ${ago(detail.updated_at)}` : ''}
                </span>
                {statusPill}
              </div>

              {isPr && pr.data && (
                <div className="mt-3.5 flex flex-wrap items-center gap-2 font-mono text-2xs text-sub">
                  <code className="rounded border border-border bg-panel2 px-1.5 py-0.5 text-foreground">
                    {pr.data.head_ref}
                  </code>
                  <span className="text-accent-text">→</span>
                  <code className="rounded border border-border bg-panel2 px-1.5 py-0.5 text-foreground">
                    {pr.data.base_ref}
                  </code>
                  {/* The counts are the obvious thing to click when you want to
                      see what changed, so they open the Files tab rather than
                      sitting there as a label. */}
                  <button
                    type="button"
                    onClick={() => setTab('files')}
                    className="ml-2 flex items-center gap-2 rounded px-1.5 py-0.5 hover:bg-panel3 hover:text-foreground"
                    title="See what changed"
                  >
                    <span>
                      {pr.data.changed_files} {pr.data.changed_files === 1 ? 'file' : 'files'}
                    </span>
                    <span className="text-added">+{pr.data.additions}</span>
                    <span className="text-removed">−{pr.data.deletions}</span>
                  </button>
                </div>
              )}

              {!isPr && issue.data && issue.data.labels.length > 0 && (
                <div className="mt-3.5 flex flex-wrap gap-1.5">
                  {issue.data.labels.map((label) => (
                    <span
                      key={label}
                      className="rounded-full border border-border bg-panel2 px-2 py-0.5 text-2xs text-sub"
                    >
                      {label}
                    </span>
                  ))}
                </div>
              )}

              {isPr && (
                <PrTabs
                  tab={tab}
                  onChange={setTab}
                  fileCount={files.data?.length}
                  commitCount={commits.data?.length}
                  commentCount={detail.comments.length}
                />
              )}

              {(!isPr || tab === 'conversation') && (
                <>
                  <section className="group/card mt-4 overflow-hidden rounded-md border border-border bg-panel2/70">
                    <div className="flex items-center gap-2 border-b border-border px-3.5 py-2.5 text-2xs font-bold text-foreground">
                      <Avatar name={detail.author} />
                      <span>{detail.author} wrote</span>
                      <span className="ml-auto font-normal text-muted-foreground">
                        {ago(detail.created_at)}
                      </span>
                      <CopyButton
                        text={`**${detail.author}** wrote ${ago(detail.created_at)}:\n\n${detail.body.trim()}`}
                        tooltip="Copy this description as markdown"
                        successMessage="Description copied as markdown"
                      />
                    </div>
                    <div className="px-4 py-3.5">
                      <MarkdownBody text={detail.body} />
                    </div>
                  </section>

                  <CommentThread
                    key={`${item.kind}-${item.number}`}
                    comments={detail.comments}
                    replyPlaceholder={isPr ? 'Write a reply…' : 'Ask for more details…'}
                    replying={m.comment.isPending}
                    onReply={(body) =>
                      m.comment.mutate({ kind: item.kind, number: item.number, body })
                    }
                  />
                </>
              )}

              {isPr && tab === 'files' && (
                <PrFileList
                  files={files.data}
                  loading={files.isPending}
                  error={files.isError}
                />
              )}

              {isPr && tab === 'commits' && (
                <PrCommitList
                  commits={commits.data}
                  loading={commits.isPending}
                  error={commits.isError}
                />
              )}
            </>
          )}
        </article>
      </div>
    </div>
  )
}
