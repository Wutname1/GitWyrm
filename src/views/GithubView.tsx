import { useEffect, useState } from 'react'
import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import { CircleDot, ExternalLink, GitPullRequest, RefreshCw, X } from 'lucide-react'
import { useQueryClient } from '@tanstack/react-query'
import { cn } from '@/lib/utils'
import { formatRelativeTime } from '@/lib/gitDisplay'
import type { GithubComment } from '@/lib/bindings'
import { Button } from '@/components/ui/button'
import { PendingIndicator } from '@/components/ui/pending-indicator'
import { Textarea } from '@/components/ui/textarea'
import {
  githubKeys,
  useGithubIssueDetail,
  useGithubMutations,
  useGithubPrDetail,
  useGithubSlug,
} from '@/hooks/useGithub'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo } from '@/stores/workspaceStore'

function ago(iso: string): string {
  const parsed = Date.parse(iso)
  return Number.isNaN(parsed) ? '' : formatRelativeTime(parsed / 1000)
}

function Avatar({ name, bot }: { name: string; bot?: boolean }) {
  return (
    <span
      className={cn(
        'flex size-6 flex-none items-center justify-center rounded-full text-[9px] font-bold',
        bot ? 'border border-border bg-panel3 text-sub' : 'bg-panel3 text-foreground'
      )}
      title={name}
    >
      {name.slice(0, 2).toUpperCase()}
    </span>
  )
}

/** GitHub-flavored markdown with styling scoped to this card. */
function MarkdownBody({ text }: { text: string }) {
  if (!text.trim()) {
    return <p className="text-xs italic text-muted-foreground">No description was written.</p>
  }
  return (
    <div
      className={cn(
        'select-text text-[12.5px] leading-relaxed text-foreground',
        '[&_p]:mb-3 [&_p:last-child]:mb-0',
        '[&_h1]:mb-2 [&_h1]:mt-4 [&_h1]:text-sm [&_h1]:font-bold',
        '[&_h2]:mb-2 [&_h2]:mt-4 [&_h2]:text-[13px] [&_h2]:font-bold',
        '[&_h3]:mb-1.5 [&_h3]:mt-3 [&_h3]:text-xs [&_h3]:font-bold',
        '[&_ul]:mb-3 [&_ul]:list-disc [&_ul]:pl-5 [&_ol]:mb-3 [&_ol]:list-decimal [&_ol]:pl-5',
        '[&_li]:mb-1',
        '[&_code]:rounded [&_code]:bg-panel3 [&_code]:px-1 [&_code]:py-px [&_code]:font-mono [&_code]:text-[11px]',
        '[&_pre]:mb-3 [&_pre]:overflow-x-auto [&_pre]:rounded-md [&_pre]:border [&_pre]:border-border [&_pre]:bg-background [&_pre]:p-2.5',
        '[&_pre_code]:bg-transparent [&_pre_code]:p-0',
        '[&_a]:text-primary [&_a]:underline-offset-2 hover:[&_a]:underline',
        '[&_blockquote]:mb-3 [&_blockquote]:border-l-2 [&_blockquote]:border-border [&_blockquote]:pl-3 [&_blockquote]:text-sub',
        '[&_img]:max-w-full [&_hr]:my-3 [&_hr]:border-border',
        '[&_table]:mb-3 [&_table]:border-collapse [&_th]:border [&_th]:border-border [&_th]:px-2 [&_th]:py-1 [&_td]:border [&_td]:border-border [&_td]:px-2 [&_td]:py-1'
      )}
    >
      <ReactMarkdown remarkPlugins={[remarkGfm]}>{text}</ReactMarkdown>
    </div>
  )
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
      <div className="flex items-center border-b border-border px-3.5 py-2.5 text-[11px] font-bold text-foreground">
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
            className="grid grid-cols-[auto_minmax(0,1fr)] gap-2.5 border-b border-border/60 py-3 last:border-b-0"
          >
            <Avatar name={c.author} bot={c.author_is_bot} />
            <div className="min-w-0">
              <div className="text-[11px]">
                <span className="font-bold text-foreground">{c.author}</span>
                <span className="ml-1.5 text-muted-foreground">{ago(c.created_at)}</span>
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
        'ml-auto rounded-full px-2 py-0.5 text-[9px] font-bold uppercase tracking-wide',
        tone === 'green' && 'bg-primary/15 text-primary',
        tone === 'violet' && 'bg-lane2/15 text-lane2',
        tone === 'red' && 'bg-removed/15 text-removed'
      )}
    >
      {label}
    </span>
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
  const pr = useGithubPrDetail(slug.data, item?.kind === 'pr' ? item.number : null)
  const issue = useGithubIssueDetail(slug.data, item?.kind === 'issue' ? item.number : null)
  const m = useGithubMutations(slug.data)

  // Jump back to the top when switching between items.
  const [scrollEl, setScrollEl] = useState<HTMLDivElement | null>(null)
  useEffect(() => {
    scrollEl?.scrollTo({ top: 0 })
  }, [item?.kind, item?.number, scrollEl])

  if (!item) return null

  const isPr = item.kind === 'pr'
  const detail = isPr ? pr.data : issue.data
  const query = isPr ? pr : issue
  const kindLabel = isPr ? 'Pull request' : 'Issue'

  const refresh = () => {
    if (!slug.data) return
    qc.invalidateQueries({
      queryKey: isPr
        ? githubKeys.pr(slug.data.owner, slug.data.repo, item.number)
        : githubKeys.issue(slug.data.owner, slug.data.repo, item.number),
    })
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
        <span className="flex size-8 flex-none items-center justify-center rounded-md border border-primary/30 bg-soft text-primary">
          {isPr ? <GitPullRequest size={15} /> : <CircleDot size={15} />}
        </span>
        <div className="min-w-0">
          <h2 className="truncate font-wordmark text-[13px] font-semibold text-foreground">
            {kindLabel} #{item.number}
          </h2>
          <p className="truncate text-[10.5px] text-sub">
            {detail ? `Updated ${ago(detail.updated_at)}` : query.isError ? 'Could not load' : 'Loading…'}
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
              <div className="flex items-center gap-2 text-[10.5px] text-muted-foreground">
                {isPr ? <GitPullRequest size={13} /> : <CircleDot size={13} />}
                <span>{kindLabel}</span>
                <span className="ml-auto font-mono">#{detail.number}</span>
              </div>
              <h3 className="mb-2.5 mt-3 select-text font-wordmark text-[22px] font-semibold leading-tight tracking-tight text-foreground">
                {detail.title}
              </h3>
              <div className="flex items-center gap-2 border-b border-border pb-4 text-[11px] text-sub">
                <Avatar name={detail.author} bot={isPr ? pr.data?.author_is_bot : false} />
                <span>
                  {detail.author} opened this · updated {ago(detail.updated_at)}
                </span>
                {statusPill}
              </div>

              {isPr && pr.data && (
                <div className="mt-3.5 flex flex-wrap items-center gap-2 font-mono text-[10px] text-sub">
                  <code className="rounded border border-border bg-panel2 px-1.5 py-0.5 text-foreground">
                    {pr.data.head_ref}
                  </code>
                  <span className="text-primary">→</span>
                  <code className="rounded border border-border bg-panel2 px-1.5 py-0.5 text-foreground">
                    {pr.data.base_ref}
                  </code>
                  <span className="ml-2">
                    {pr.data.changed_files} {pr.data.changed_files === 1 ? 'file' : 'files'}
                  </span>
                  <span className="text-added">+{pr.data.additions}</span>
                  <span className="text-removed">−{pr.data.deletions}</span>
                </div>
              )}

              {!isPr && issue.data && issue.data.labels.length > 0 && (
                <div className="mt-3.5 flex flex-wrap gap-1.5">
                  {issue.data.labels.map((label) => (
                    <span
                      key={label}
                      className="rounded-full border border-border bg-panel2 px-2 py-0.5 text-[9.5px] text-sub"
                    >
                      {label}
                    </span>
                  ))}
                </div>
              )}

              <section className="mt-4 overflow-hidden rounded-md border border-border bg-panel2/70">
                <div className="flex items-center gap-2 border-b border-border px-3.5 py-2.5 text-[11px] font-bold text-foreground">
                  <Avatar name={detail.author} />
                  <span>{detail.author} wrote</span>
                  <span className="ml-auto font-normal text-muted-foreground">
                    {ago(detail.created_at)}
                  </span>
                </div>
                <div className="px-4 py-3.5">
                  <MarkdownBody text={detail.body} />
                </div>
              </section>

              <CommentThread
                comments={detail.comments}
                replyPlaceholder={isPr ? 'Write a reply…' : 'Ask for more details…'}
                replying={m.comment.isPending}
                onReply={(body) =>
                  m.comment.mutate({ kind: item.kind, number: item.number, body })
                }
              />
            </>
          )}
        </article>
      </div>
    </div>
  )
}
