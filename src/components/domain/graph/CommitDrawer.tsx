import { X } from 'lucide-react'
import { toast } from 'sonner'
import { copyToClipboard } from '@/lib/clipboard'
import { Button } from '@/components/ui/button'
import { TooltipButton } from '@/components/ui/tooltip'
import { authorColor, formatCommitTime } from '@/lib/gitDisplay'
import { useCommitDetail } from '@/hooks/useGitQueries'
import { useUiStore } from '@/stores/uiStore'
import { Avatar } from './Avatar'
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
    ? d.parent_shas.map((p) => p.slice(0, 7)).join(', ')
    : '(root)'
  const adds = d.files.reduce((a, f) => a + f.additions, 0)
  const dels = d.files.reduce((a, f) => a + f.deletions, 0)

  return (
    <div className="flex h-[212px] min-h-0 flex-none flex-col border-t border-border bg-panel">
      <div className="flex flex-none items-center gap-2.5 border-b border-border px-3.5 py-[9px]">
        <Avatar initials={initials(d.author_name)} color={color} email={d.author_email} size="md" />
        <div className="min-w-0 flex-1">
          <div className="overflow-hidden text-ellipsis whitespace-nowrap text-[12.5px] font-semibold text-foreground">
            {d.summary}
          </div>
          <div className="text-[10.5px] text-muted-foreground">
            {d.author_name} committed · {formatCommitTime(d.time)} · parents{' '}
            <span className="font-mono">{parents}</span>
          </div>
        </div>
        <span className="rounded-[5px] border border-border bg-panel2 px-2 py-[3px] font-mono text-[10.5px] text-sub">
          {d.sha.slice(0, 7)}
        </span>
        <Button
          variant="secondary"
          size="sm"
          className="h-auto rounded border-border bg-panel3 px-[7px] py-0.5 text-[10px] text-sub"
          onClick={() => void copyToClipboard(d.sha, `Copied ${d.sha.slice(0, 7)}`)}
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
      <div className="flex flex-none items-center gap-3.5 border-b border-border px-3.5 py-[5px] text-[10.5px] text-sub">
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
