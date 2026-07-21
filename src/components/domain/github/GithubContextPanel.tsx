import { useState } from 'react'
import {
  ArrowLeftRight,
  Check,
  ChevronDown,
  CircleDot,
  ExternalLink,
  GitPullRequest,
} from 'lucide-react'
import { Button } from '@/components/ui/button'
import { PendingIndicator } from '@/components/ui/pending-indicator'
import { Input } from '@/components/ui/input'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { ConfirmDialog } from '@/components/modals/ConfirmDialog'
import { FormDialog } from '@/components/ui/form-dialog'
import { useGitMutations } from '@/hooks/useGitMutations'
import { useBranches } from '@/hooks/useGitQueries'
import {
  useGithubIssueDetail,
  useGithubMutations,
  useGithubPrDetail,
  useGithubSlug,
} from '@/hooks/useGithub'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo } from '@/stores/workspaceStore'

type MergeMethod = 'merge' | 'squash' | 'rebase'

const MERGE_METHODS: Record<MergeMethod, { label: string; description: string }> = {
  merge: {
    label: 'Merge pull request',
    description: 'Keep every commit and add a merge commit.',
  },
  squash: {
    label: 'Squash and merge',
    description: 'Combine all commits into one commit.',
  },
  rebase: {
    label: 'Rebase and merge',
    description: 'Replay each commit without a merge commit.',
  },
}

async function openExternal(url: string) {
  const { openUrl } = await import('@tauri-apps/plugin-opener')
  await openUrl(url)
}

/** Turns an issue title into a usable branch name: `issue/218-drag-preview`. */
function suggestBranchName(number: number, title: string): string {
  const slug = title
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .split('-')
    .slice(0, 4)
    .join('-')
  return slug ? `issue/${number}-${slug}` : `issue/${number}`
}

function PanelShell({
  icon,
  title,
  status,
  children,
}: {
  icon: React.ReactNode
  title: string
  status: string
  children: React.ReactNode
}) {
  return (
    <div className="grid flex-none gap-2 border-b-2 border-border bg-panel2/60 px-3 py-2.5">
      <div className="flex items-center gap-2">
        <span className="text-accent-text">{icon}</span>
        <span className="text-2xs font-bold text-foreground">{title}</span>
        <span className="ml-auto text-2xs uppercase tracking-wide text-muted-foreground">
          {status}
        </span>
      </div>
      {children}
    </div>
  )
}

function PrPanel({ number }: { number: number }) {
  const repo = useActiveRepo()
  const slug = useGithubSlug(repo?.id ?? null)
  const pr = useGithubPrDetail(slug.data, number)
  const gh = useGithubMutations(slug.data)
  const git = useGitMutations(repo?.id ?? null)
  const branches = useBranches(repo?.id ?? null)

  const [method, setMethod] = useState<MergeMethod>('merge')
  const [confirmMerge, setConfirmMerge] = useState(false)

  if (!pr.data) {
    return (
      <PanelShell icon={<GitPullRequest size={14} />} title={`Pull request #${number}`} status="">
        <div className="flex items-center gap-2 py-1 text-2xs text-muted-foreground">
          <PendingIndicator /> Loading…
        </div>
      </PanelShell>
    )
  }

  const detail = pr.data
  const merged = detail.merged
  const closed = !merged && detail.state === 'closed'
  const canMerge = !merged && !closed && !detail.draft
  const onBranch = branches.data?.local.find((b) => b.is_head)?.name === detail.head_ref

  // The head branch may already exist locally; otherwise switch to the
  // remote-tracking ref, which lands on a fresh local tracking branch.
  const useBranch = () => {
    const hasLocal = branches.data?.local.some((b) => b.name === detail.head_ref)
    const target = hasLocal ? detail.head_ref : `origin/${detail.head_ref}`
    git.fetch.mutate(undefined, {
      onSuccess: () => git.checkout.mutate(target),
    })
  }

  const note = merged
    ? 'This pull request was added to the project.'
    : closed
      ? 'This pull request was closed without merging.'
      : detail.draft
        ? 'Still a draft; the author is not done yet.'
        : detail.mergeable === false
          ? 'Has conflicts that need to be resolved on GitHub.'
          : 'Open and ready for review.'

  return (
    <PanelShell
      icon={<GitPullRequest size={14} />}
      title={`Pull request #${number}`}
      status={merged ? 'Merged' : closed ? 'Closed' : detail.draft ? 'Draft' : 'Open'}
    >
      <p className="text-2xs leading-snug text-sub">{note}</p>

      <div className="flex overflow-hidden rounded-md border border-primary">
        <Button
          size="sm"
          className="h-8 flex-1 rounded-none text-xs font-bold"
          disabled={!canMerge || gh.mergePr.isPending}
          aria-busy={gh.mergePr.isPending || undefined}
          onClick={() => setConfirmMerge(true)}
        >
          {gh.mergePr.isPending ? <PendingIndicator /> : null}
          {merged ? 'Pull request merged' : MERGE_METHODS[method].label}
        </Button>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              size="sm"
              className="h-8 w-8 rounded-none border-l border-primary-foreground/25 px-0"
              disabled={!canMerge || gh.mergePr.isPending}
              aria-label="Choose merge method"
            >
              <ChevronDown size={14} />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end" className="w-64">
            {(Object.keys(MERGE_METHODS) as MergeMethod[]).map((key) => (
              <DropdownMenuItem key={key} onSelect={() => setMethod(key)}>
                <Check size={13} className={key === method ? 'text-accent-text' : 'opacity-0'} />
                <div>
                  <div className="text-xs font-semibold">{MERGE_METHODS[key].label}</div>
                  <div className="text-2xs text-muted-foreground">
                    {MERGE_METHODS[key].description}
                  </div>
                </div>
              </DropdownMenuItem>
            ))}
          </DropdownMenuContent>
        </DropdownMenu>
      </div>

      <div className="grid grid-cols-2 gap-1.5">
        <Button
          variant="secondary"
          size="sm"
          className="h-7 text-2xs"
          disabled={merged || closed || gh.approvePr.isPending}
          aria-busy={gh.approvePr.isPending || undefined}
          onClick={() => gh.approvePr.mutate(number)}
        >
          {gh.approvePr.isPending ? <PendingIndicator /> : <Check size={12} />}
          Approve
        </Button>
        <Button
          variant="secondary"
          size="sm"
          className="h-7 text-2xs"
          disabled={onBranch || git.fetch.isPending || git.checkout.isPending}
          aria-busy={git.fetch.isPending || git.checkout.isPending || undefined}
          onClick={useBranch}
          tooltip={
            onBranch ? `You are already on ${detail.head_ref}` : `Switch to ${detail.head_ref}`
          }
        >
          {git.fetch.isPending || git.checkout.isPending ? (
            <PendingIndicator />
          ) : (
            <ArrowLeftRight size={12} />
          )}
          {onBranch ? 'On this branch' : 'Use branch'}
        </Button>
      </div>
      <Button
        variant="ghost"
        size="sm"
        className="h-7 text-2xs text-sub"
        onClick={() => void openExternal(detail.html_url)}
      >
        <ExternalLink size={12} />
        Open on GitHub
      </Button>

      <ConfirmDialog
        open={confirmMerge}
        onOpenChange={setConfirmMerge}
        title={`${MERGE_METHODS[method].label}?`}
        description={
          <>
            {MERGE_METHODS[method].description} GitHub will close pull request{' '}
            <span className="font-mono text-foreground">#{number}</span> and add{' '}
            <span className="font-mono text-foreground">{detail.head_ref}</span> to{' '}
            <span className="font-mono text-foreground">{detail.base_ref}</span>.
          </>
        }
        confirmLabel={MERGE_METHODS[method].label}
        pendingLabel="Adding to the project…"
        pending={gh.mergePr.isPending}
        keepOpenOnConfirm
        onConfirm={() =>
          gh.mergePr.mutate({ number, method }, { onSettled: () => setConfirmMerge(false) })
        }
      />
    </PanelShell>
  )
}

function IssuePanel({ number }: { number: number }) {
  const repo = useActiveRepo()
  const slug = useGithubSlug(repo?.id ?? null)
  const issue = useGithubIssueDetail(slug.data, number)
  const gh = useGithubMutations(slug.data)
  const git = useGitMutations(repo?.id ?? null)
  const branches = useBranches(repo?.id ?? null)

  const [startOpen, setStartOpen] = useState(false)
  const [branchName, setBranchName] = useState('')
  const [confirmClose, setConfirmClose] = useState(false)

  if (!issue.data) {
    return (
      <PanelShell icon={<CircleDot size={14} />} title={`Issue #${number}`} status="">
        <div className="flex items-center gap-2 py-1 text-2xs text-muted-foreground">
          <PendingIndicator /> Loading…
        </div>
      </PanelShell>
    )
  }

  const detail = issue.data
  const closed = detail.state === 'closed'
  const suggested = suggestBranchName(number, detail.title)
  const trimmedName = branchName.trim()
  const nameTaken = (branches.data?.local ?? []).some((b) => b.name === trimmedName)

  return (
    <PanelShell
      icon={<CircleDot size={14} />}
      title={`Issue #${number}`}
      status={closed ? 'Closed' : 'Open'}
    >
      <p className="text-2xs leading-snug text-sub">
        {closed
          ? 'This issue is closed. It can still be read and reopened on GitHub.'
          : detail.assignee
            ? `${detail.assignee} is assigned to this issue.`
            : 'No one is assigned to this issue yet.'}
      </p>

      <Button
        size="sm"
        className="h-8 text-xs font-bold"
        disabled={closed}
        onClick={() => {
          setBranchName(suggested)
          setStartOpen(true)
        }}
      >
        Start work
      </Button>
      <div className="grid grid-cols-2 gap-1.5">
        <Button
          variant="secondary"
          size="sm"
          className="h-7 text-2xs"
          onClick={() => void openExternal(detail.html_url)}
        >
          <ExternalLink size={12} />
          On GitHub
        </Button>
        <Button
          variant="secondary"
          size="sm"
          className="h-7 text-2xs text-removed hover:text-removed"
          disabled={closed || gh.closeIssue.isPending}
          onClick={() => setConfirmClose(true)}
        >
          Close issue
        </Button>
      </div>

      <FormDialog
        open={startOpen}
        onOpenChange={setStartOpen}
        title={`Start work on issue #${number}`}
        submitLabel="Create branch and start"
        pendingLabel="Creating branch…"
        canSubmit={trimmedName.length > 0 && !nameTaken}
        pending={git.createBranch.isPending}
        onSubmit={() =>
          git.createBranch.mutate(
            { name: trimmedName, checkout: true },
            { onSuccess: () => setStartOpen(false) }
          )
        }
      >
        <p className="text-xs leading-relaxed text-sub">
          GitWyrm will make a branch for this issue and switch to it. Your files will stay as they
          are.
        </p>
        <div className="grid gap-1.5">
          <label className="text-2xs text-muted-foreground" htmlFor="issue-branch-name">
            Branch name
          </label>
          <Input
            id="issue-branch-name"
            value={branchName}
            onChange={(e) => setBranchName(e.target.value)}
            className="h-auto bg-background py-1.5 font-mono text-xs"
            spellCheck={false}
            autoFocus
          />
          {nameTaken && (
            <p className="text-2xs text-removed">A branch with this name already exists.</p>
          )}
        </div>
      </FormDialog>

      <ConfirmDialog
        open={confirmClose}
        onOpenChange={setConfirmClose}
        destructive
        title={`Close issue #${number}?`}
        description={
          <>
            This marks <span className="text-foreground">{detail.title}</span> as closed. People
            can still read it, comment on it, and reopen it later.
          </>
        }
        confirmLabel="Close issue"
        pendingLabel="Closing issue…"
        pending={gh.closeIssue.isPending}
        keepOpenOnConfirm
        onConfirm={() =>
          gh.closeIssue.mutate(number, { onSettled: () => setConfirmClose(false) })
        }
      />
    </PanelShell>
  )
}

/**
 * GitHub actions for the selected PR or issue, docked above the Changes list
 * so the local workflow stays visible while working GitHub items.
 */
export function GithubContextPanel() {
  const item = useUiStore((s) => s.githubItem)
  const centerView = useUiStore((s) => s.centerView)
  if (!item || centerView !== 'github') return null
  return item.kind === 'pr' ? <PrPanel number={item.number} /> : <IssuePanel number={item.number} />
}
