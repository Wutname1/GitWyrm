import { useQueries } from '@tanstack/react-query'
import { X } from 'lucide-react'
import { copyToClipboard } from '@/lib/clipboard'
import { commands, type CommitDetail, type FileChange } from '@/lib/bindings'
import { keys, unwrap } from '@/lib/queryKeys'
import { Button } from '@/components/ui/button'
import { TooltipButton } from '@/components/ui/tooltip'
import { plural, shortSha } from '@/lib/gitDisplay'
import { useUiStore } from '@/stores/uiStore'
import { FileChangeRow } from '../FileChangeRow'

/** A file's changes summed across the selected commits. */
interface CombinedFile {
  file: FileChange
  /** Newest selected commit touching the file; its diff opens on click. */
  sha: string
}

/**
 * Merge each commit's file list into one combined list: line counts add up
 * per path, and the status shown is the one from the newest commit that
 * touched the file. `details` arrives newest-first (graph order).
 */
function combineFiles(details: CommitDetail[]): CombinedFile[] {
  const byPath = new Map<string, CombinedFile>()
  for (const d of details) {
    for (const f of d.files) {
      const existing = byPath.get(f.path)
      if (existing) {
        existing.file = {
          ...existing.file,
          additions: existing.file.additions + f.additions,
          deletions: existing.file.deletions + f.deletions,
        }
      } else {
        byPath.set(f.path, { file: { ...f }, sha: d.sha })
      }
    }
  }
  return [...byPath.values()].sort((a, b) => a.file.path.localeCompare(b.file.path))
}

/**
 * The drawer shown when 2+ commits are selected: one combined list of every
 * file those commits changed, with summed line counts. Clicking a file opens
 * its diff from the newest selected commit that touched it.
 */
export function MultiCommitDrawer({ repoId, shas }: { repoId: string; shas: string[] }) {
  const selectCommit = useUiStore((s) => s.selectCommit)
  const openDiff = useUiStore((s) => s.openDiff)
  const diffRequest = useUiStore((s) => s.diffRequest)

  const results = useQueries({
    queries: shas.map((sha) => ({
      queryKey: keys.commitDetail(repoId, sha),
      queryFn: async () => unwrap(await commands.getCommitDetail(repoId, sha)),
    })),
  })

  const failed = results.find((r) => r.isError)
  if (failed) {
    return (
      <div className="flex h-[212px] flex-none items-center justify-center border-t border-border bg-panel text-xs text-removed">
        {(failed.error as Error | null)?.message ?? 'Failed to load commits'}
      </div>
    )
  }
  if (results.some((r) => r.isLoading || !r.data)) {
    return (
      <div className="flex h-[212px] flex-none items-center justify-center border-t border-border bg-panel text-xs text-muted-foreground">
        Loading {plural(shas.length, 'commit')}…
      </div>
    )
  }

  const details = results.map((r) => r.data!)
  const combined = combineFiles(details)
  const adds = combined.reduce((a, c) => a + c.file.additions, 0)
  const dels = combined.reduce((a, c) => a + c.file.deletions, 0)
  const newest = shas[0]
  const oldest = shas[shas.length - 1]

  // Highlight the row whose diff is on screen, but only when that diff came
  // from one of the selected commits.
  const shaSet = new Set(shas)
  const openPath =
    diffRequest?.source.kind === 'commit' && shaSet.has(diffRequest.source.sha)
      ? diffRequest.path
      : null

  return (
    <div className="flex h-[212px] min-h-0 flex-none flex-col border-t border-border bg-panel">
      <div className="flex flex-none items-center gap-2.5 border-b border-border px-3.5 py-[9px]">
        <div className="min-w-0 flex-1">
          <div className="overflow-hidden text-ellipsis whitespace-nowrap text-[0.78125rem] font-semibold text-foreground">
            {plural(shas.length, 'commit')} selected
          </div>
          <div className="text-2xs text-muted-foreground">
            All their changes combined. Right-click a selected row in the graph for actions.
          </div>
        </div>
        <span className="rounded-[5px] border border-border bg-panel2 px-2 py-[3px] font-mono text-2xs text-sub">
          {shortSha(oldest)} … {shortSha(newest)}
        </span>
        <Button
          variant="secondary"
          size="sm"
          className="h-auto rounded border-border bg-panel3 px-[7px] py-0.5 text-2xs text-sub"
          onClick={() => void copyToClipboard(shas.join('\n'), `Copied ${plural(shas.length, 'SHA')}`)}
        >
          Copy SHAs
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
        <span className="font-semibold">{plural(combined.length, 'file')} changed</span>
        <span className="font-mono text-added">+{adds}</span>
        <span className="font-mono text-removed">-{dels}</span>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto py-1">
        {combined.map((c) => (
          <FileChangeRow
            key={c.file.path}
            file={c.file}
            mono
            nameClassName="text-sub"
            menuSha={c.sha}
            active={openPath === c.file.path}
            onOpen={() => openDiff({ path: c.file.path, source: { kind: 'commit', sha: c.sha } })}
          />
        ))}
      </div>
    </div>
  )
}
