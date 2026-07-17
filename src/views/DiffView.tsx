import { useFileDiff } from '@/hooks/useGitQueries'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo } from '@/stores/workspaceStore'
import { FileHeader } from '@/components/domain/diff/FileHeader'
import { DiffLineRow } from '@/components/domain/diff/DiffLineRow'

export function DiffView() {
  const repo = useActiveRepo()
  const request = useUiStore((s) => s.diffRequest)
  const diff = useFileDiff(repo?.id ?? null, request?.path ?? null, request?.source ?? null)

  if (!request) return null

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <FileHeader
        request={request}
        additions={diff.data?.additions ?? 0}
        deletions={diff.data?.deletions ?? 0}
      />
      <div className="min-h-0 flex-1 overflow-auto pb-5 font-mono text-[11.5px] leading-[1.8]">
        {diff.isLoading && (
          <div className="p-4 text-xs text-muted-foreground">Loading diff…</div>
        )}
        {diff.isError && (
          <div className="p-4 text-xs text-removed">{(diff.error as Error).message}</div>
        )}
        {diff.data?.binary && (
          <div className="p-4 text-xs text-muted-foreground">Binary file — no text diff.</div>
        )}
        {diff.data && !diff.data.binary && diff.data.lines.length === 0 && (
          <div className="p-4 text-xs text-muted-foreground">No changes to show.</div>
        )}
        {diff.data?.lines.map((line, i) => <DiffLineRow key={i} line={line} />)}
      </div>
    </div>
  )
}
