import { useMemo, useState } from 'react'
import { useQueries } from '@tanstack/react-query'
import { ChevronDown, ChevronRight, X } from 'lucide-react'
import { copyToClipboard } from '@/lib/clipboard'
import { commands, type CommitDetail, type FileChange } from '@/lib/bindings'
import { keys, unwrap } from '@/lib/queryKeys'
import { Button } from '@/components/ui/button'
import { TooltipButton } from '@/components/ui/tooltip'
import { authorColor, formatCommitTime, plural, shortSha } from '@/lib/gitDisplay'
import { cn } from '@/lib/utils'
import { useUiStore } from '@/stores/uiStore'
import {
  DEFAULT_DRAWER_LIST_WIDTH,
  MAX_DRAWER_LIST_WIDTH,
  MIN_DRAWER_LIST_WIDTH,
  useWorkspaceStore,
} from '@/stores/workspaceStore'
import { ResizeHandle } from '@/components/ui/ResizeHandle'
import { Avatar } from './Avatar'
import { FileChangeRow } from '../FileChangeRow'

/** A file's changes summed across the selected commits. */
interface CombinedFile {
  file: FileChange
  /** Newest selected commit touching the file; its diff opens on click. */
  sha: string
}

/**
 * Merge each commit's file list into one combined list: line counts add up per
 * path, and the status reads as the whole selection's effect -- a file the
 * selection itself added shows as added even when later selected commits kept
 * editing it. `details` arrives newest-first (graph order).
 */
function combineFiles(details: CommitDetail[]): CombinedFile[] {
  const byPath = new Map<string, CombinedFile>()
  for (const d of details) {
    for (const f of d.files) {
      const existing = byPath.get(f.path)
      if (existing) {
        existing.file = {
          ...existing.file,
          // This commit is older than the ones already merged, so its status
          // wins only when it introduced the file (and it wasn't deleted since).
          status:
            f.status === 'A' && existing.file.status !== 'D' ? 'A' : existing.file.status,
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

interface TreeNode {
  name: string
  path: string
  directories: Map<string, TreeNode>
  files: CombinedFile[]
}

function buildTree(files: CombinedFile[]): TreeNode {
  const root: TreeNode = { name: '', path: '', directories: new Map(), files: [] }
  for (const entry of files) {
    const segments = entry.file.path.replaceAll('\\', '/').split('/').filter(Boolean)
    segments.pop()
    let node = root
    for (const segment of segments) {
      const path = node.path ? `${node.path}/${segment}` : segment
      let child = node.directories.get(segment)
      if (!child) {
        child = { name: segment, path, directories: new Map(), files: [] }
        node.directories.set(segment, child)
      }
      node = child
    }
    node.files.push(entry)
  }
  return root
}

/**
 * The drawer shown when 2+ commits are selected: the commits themselves on the
 * left, and one combined list of every file they changed on the right, with
 * summed line counts. Clicking a commit narrows the selection to just it;
 * clicking a file opens its diff from the newest selected commit touching it.
 */
export function MultiCommitDrawer({ repoId, shas }: { repoId: string; shas: string[] }) {
  const selectCommit = useUiStore((s) => s.selectCommit)
  const openDiff = useUiStore((s) => s.openDiff)
  const diffRequest = useUiStore((s) => s.diffRequest)
  const [view, setView] = useState<'path' | 'tree'>('path')
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set())
  const listWidth = useWorkspaceStore((s) => s.drawerListWidth)
  const setListWidth = useWorkspaceStore((s) => s.setDrawerListWidth)

  const results = useQueries({
    queries: shas.map((sha) => ({
      queryKey: keys.commitDetail(repoId, sha),
      queryFn: async () => unwrap(await commands.getCommitDetail(repoId, sha)),
    })),
  })
  const loaded = !results.some((r) => r.isLoading || (!r.data && !r.isError))
  const details = useMemo(
    () => (loaded ? results.flatMap((r) => (r.data ? [r.data] : [])) : []),
    [loaded, results]
  )
  const combined = useMemo(() => combineFiles(details), [details])

  const failed = results.find((r) => r.isError)
  if (failed) {
    return (
      <div className="flex h-full flex-none items-center justify-center border-t border-border bg-panel text-xs text-removed">
        {(failed.error as Error | null)?.message ?? 'Failed to load commits'}
      </div>
    )
  }
  if (!loaded) {
    return (
      <div className="flex h-full flex-none items-center justify-center border-t border-border bg-panel text-xs text-muted-foreground">
        Loading {plural(shas.length, 'commit')}…
      </div>
    )
  }

  const adds = combined.reduce((a, c) => a + c.file.additions, 0)
  const dels = combined.reduce((a, c) => a + c.file.deletions, 0)
  const modifiedCount = combined.filter((c) => c.file.status !== 'A' && c.file.status !== 'D').length
  const addedCount = combined.filter((c) => c.file.status === 'A').length
  const deletedCount = combined.filter((c) => c.file.status === 'D').length

  // Highlight the row whose diff is on screen, but only when that diff came
  // from one of the selected commits.
  const shaSet = new Set(shas)
  const openPath =
    diffRequest?.source.kind === 'commit' && shaSet.has(diffRequest.source.sha)
      ? diffRequest.path
      : null

  const fileRow = (c: CombinedFile, displayPath?: string, depth?: number) => (
    <FileChangeRow
      key={c.file.path}
      file={c.file}
      displayPath={displayPath}
      treeDepth={depth}
      mono
      nameClassName="text-sub"
      menuSha={c.sha}
      active={openPath === c.file.path}
      onOpen={() => openDiff({ path: c.file.path, source: { kind: 'commit', sha: c.sha } })}
    />
  )

  const renderTree = (node: TreeNode, depth: number): React.ReactNode => (
    <>
      {[...node.directories.values()]
        .sort((a, b) => a.name.localeCompare(b.name))
        .map((dir) => {
          const isCollapsed = collapsed.has(dir.path)
          return (
            <div key={dir.path}>
              <button
                type="button"
                onClick={() =>
                  setCollapsed((cur) => {
                    const next = new Set(cur)
                    if (next.has(dir.path)) next.delete(dir.path)
                    else next.add(dir.path)
                    return next
                  })
                }
                style={{ paddingLeft: 10 + depth * 14 }}
                className="flex h-6 w-full items-center gap-1.5 pr-3.5 text-left text-xs font-medium text-sub hover:bg-panel2 hover:text-foreground"
              >
                {isCollapsed ? (
                  <ChevronRight aria-hidden className="size-3 flex-none" />
                ) : (
                  <ChevronDown aria-hidden className="size-3 flex-none" />
                )}
                <span className="min-w-0 flex-1 overflow-hidden text-ellipsis whitespace-nowrap">
                  {dir.name}
                </span>
              </button>
              {!isCollapsed && renderTree(dir, depth + 1)}
            </div>
          )
        })}
      {[...node.files]
        .sort((a, b) => a.file.path.localeCompare(b.file.path))
        .map((c) => fileRow(c, c.file.path.replaceAll('\\', '/').split('/').pop(), depth))}
    </>
  )

  return (
    <div className="flex h-full min-h-0 flex-none flex-col border-t border-border bg-panel">
      <div className="flex flex-none items-center gap-2.5 border-b border-border px-3.5 py-[7px]">
        <div className="min-w-0 flex-1 text-[0.78125rem] font-semibold text-foreground">
          Viewing the merged changes of {plural(shas.length, 'commit')}
        </div>
        <Button
          variant="secondary"
          size="sm"
          className="h-auto rounded border-border bg-panel3 px-[7px] py-0.5 text-2xs text-sub"
          onClick={() =>
            void copyToClipboard(shas.join('\n'), `Copied ${plural(shas.length, 'SHA')}`)
          }
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
      <div className="flex min-h-0 flex-1">
        {/* The selected commits, newest first. Click one to view just it. */}
        <div
          className="relative flex min-h-0 flex-none flex-col"
          style={{ width: listWidth }}
        >
          <div className="min-h-0 flex-1 overflow-y-auto border-r border-border py-1">
          {details.map((d) => (
            <TooltipButton
              key={d.sha}
              type="button"
              onClick={() => selectCommit(d.sha)}
              tooltip="View just this commit"
              className="flex w-full items-center gap-2 px-3 py-[3px] text-left hover:bg-panel2"
            >
              <Avatar
                initials={d.author_name.slice(0, 2).toUpperCase()}
                color={authorColor(d.author_email || d.author_name)}
                email={d.author_email}
              />
              <span className="min-w-0 flex-1">
                <span className="block overflow-hidden text-ellipsis whitespace-nowrap text-xs text-foreground">
                  {d.summary}
                </span>
                <span className="block text-2xs text-muted-foreground">
                  {formatCommitTime(d.time)} · {d.author_name}
                </span>
              </span>
              <span className="flex-none font-mono text-2xs text-muted-foreground">
                {shortSha(d.sha)}
              </span>
            </TooltipButton>
          ))}
          </div>
          <ResizeHandle
            ariaLabel="Resize commit list"
            value={listWidth}
            min={MIN_DRAWER_LIST_WIDTH}
            max={MAX_DRAWER_LIST_WIDTH}
            defaultValue={DEFAULT_DRAWER_LIST_WIDTH}
            onChange={setListWidth}
            className="-right-1"
          />
        </div>
        {/* Combined file list with the selection's overall counts. */}
        <div className="flex min-h-0 min-w-0 flex-1 flex-col">
          <div className="flex flex-none items-center gap-3 border-b border-border px-3.5 py-[5px] text-2xs">
            <span className="text-modified">{modifiedCount} modified</span>
            <span className="text-added">+{addedCount} added</span>
            <span className="text-removed">-{deletedCount} deleted</span>
            <span className="ml-auto font-mono text-added">+{adds}</span>
            <span className="font-mono text-removed">-{dels}</span>
            <span className="ml-1 flex overflow-hidden rounded border border-border">
              {(['path', 'tree'] as const).map((mode) => (
                <button
                  key={mode}
                  type="button"
                  onClick={() => setView(mode)}
                  className={cn(
                    'px-2 py-0.5 capitalize',
                    view === mode
                      ? 'bg-panel3 text-foreground'
                      : 'text-sub hover:bg-panel2 hover:text-foreground'
                  )}
                >
                  {mode}
                </button>
              ))}
            </span>
          </div>
          <div className="min-h-0 flex-1 overflow-y-auto py-1">
            {view === 'path'
              ? combined.map((c) => fileRow(c))
              : renderTree(buildTree(combined), 0)}
          </div>
        </div>
      </div>
    </div>
  )
}
