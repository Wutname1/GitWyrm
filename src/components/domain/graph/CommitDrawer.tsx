import { X } from 'lucide-react'
import { toast } from 'sonner'
import { copyToClipboard } from '@/lib/clipboard'
import { Button } from '@/components/ui/button'
import { TooltipButton } from '@/components/ui/tooltip'
import { authorColor, formatCommitTime, shortSha } from '@/lib/gitDisplay'
import { useCommitDetail } from '@/hooks/useGitQueries'
import { useUiStore } from '@/stores/uiStore'
import { Avatar } from './Avatar'
import { AuthorHoverCard } from './AuthorHoverCard'
import { FileChangeRow } from '../FileChangeRow'

function initials(name: string): string {
  const words = name.split(/\s+/).filter(Boolean)
  if (words.length >= 2) return (words[0][0] + words[words.length - 1][0]).toUpperCase()
  return name.slice(0, 2).toUpperCase()
}

export function CommitDrawer({ repoId, sha }: { repoId: string; sha: string }) {
  const selectCommit = useUiStore((s) => s.selectCommit)
  const openDiff = useUiStore((s) => s.openDiff)
  const detail = useCommitDetail(repoId, sha)

  if (detail.isLoading) {
    return (
      <div className="flex h-[212px] flex-none items-center justify-center border-t border-border bg-panel text-xs text-muted-foreground">
        Loading commit…
      </div>
    )
  }
  if (detail.isError || !detail.data) {
    return (
      <div className="flex h-[212px] flex-none items-center justify-center border-t border-border bg-panel text-xs text-removed">
        {(detail.error as Error | null)?.message ?? 'Failed to load commit'}
      </div>
    )
  }

  const d = detail.data
  const color = authorColor(d.author_name)
  const parents = d.parent_shas.length
    ? d.parent_shas.map((p) => shortSha(p)).join(', ')
    : '(root)'
  const adds = d.files.reduce((a, f) => a + f.additions, 0)
  const dels = d.files.reduce((a, f) => a + f.deletions, 0)

  return (
    <div className="flex h-[212px] min-h-0 flex-none flex-col border-t border-border bg-panel">
      <div className="flex flex-none items-center gap-2.5 border-b border-border px-3.5 py-[9px]">
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
          <div className="overflow-hidden text-ellipsis whitespace-nowrap text-[0.78125rem] font-semibold text-foreground">
            {d.summary}
          </div>
          <div className="text-2xs text-muted-foreground">
            <AuthorHoverCard
              name={d.author_name}
              email={d.author_email}
              initials={initials(d.author_name)}
            >
              <span className="cursor-default hover:text-foreground">{d.author_name}</span>
            </AuthorHoverCard>{' '}
            committed · {formatCommitTime(d.time)} · parents{' '}
            <span className="font-mono">{parents}</span>
          </div>
        </div>
        <span className="rounded-[5px] border border-border bg-panel2 px-2 py-[3px] font-mono text-2xs text-sub">
          {shortSha(d.sha)}
        </span>
        <Button
          variant="secondary"
          size="sm"
          className="h-auto rounded border-border bg-panel3 px-[7px] py-0.5 text-2xs text-sub"
          onClick={() => void copyToClipboard(d.sha, `Copied ${shortSha(d.sha)}`)}
        >
          Copy SHA
        </Button>
        <TooltipButton
          onClick={() => selectCommit(null)}
          tooltip="Close"
          className="flex size-6 flex-none items-center justify-center rounded-[5px] border border-border bg-panel2 text-xs text-sub hover:border-muted-foreground hover:bg-panel3"
        >
          <X size={12} />
        </TooltipButton>
      </div>
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
            onOpen={() => openDiff({ path: f.path, source: { kind: 'commit', sha: d.sha } })}
          />
        ))}
      </div>
    </div>
  )
}
