import { useCallback, useLayoutEffect, useMemo, useRef, useState, type ReactNode, type RefObject } from 'react'
import { useVirtualizer } from '@tanstack/react-virtual'
import { Archive, ChevronDown, ChevronRight, ChevronsDownUp, ChevronsUpDown, FolderOpen, MinusCircle, Pencil, Plus, PlusCircle, Trash2 } from 'lucide-react'
import type { FileChange, StatusCode } from '@/lib/bindings'
import { statusColor } from '@/lib/gitDisplay'
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuLabel,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from '@/components/ui/context-menu'
import { TooltipHint } from '@/components/ui/tooltip'
import { PendingMenuItem } from '@/components/ui/pending-menu-item'
import { PendingIndicator } from '@/components/ui/pending-indicator'
import { ConfirmDialog } from '@/components/modals/ConfirmDialog'
import { IgnoreMenuItems } from '@/components/domain/commit-form/IgnoreMenuItems'
import { useGitMutations } from '@/hooks/useGitMutations'
import { changeTreeKey, useActiveRepo, useWorkspaceStore, type ChangesViewMode } from '@/stores/workspaceStore'
import { cn } from '@/lib/utils'
import {
  buildChangeTree,
  filesByFolder,
  flatRows,
  flattenVisible,
  folderKeys,
  type ChangeTreeNode,
} from '@/lib/changeTreeRows'

interface FileChangeTreeProps {
  files: FileChange[]
  /** All staged and unstaged files, used by recursive folder discard. */
  allFiles: FileChange[]
  treeId: string
  staged: boolean
  /** 'list' skips folder grouping and shows one flat row per file. */
  viewMode?: ChangesViewMode
  operationsDisabled?: boolean
  mutations: Pick<
    ReturnType<typeof useGitMutations>,
    'stageFiles' | 'unstageFiles' | 'discardFiles' | 'stashFolder' | 'openFolder'
  >
  /** `depth` is undefined in list view, where rows are not tree items. */
  renderFile: (file: FileChange, name: string, depth: number | undefined) => ReactNode
  /**
   * The element that scrolls this list. Only the rows on screen in it are
   * drawn, so a thousand changed files cost about the same as thirty.
   */
  scrollRef: RefObject<HTMLDivElement | null>
}

interface PendingFolderDiscard {
  name: string
  path: string
  paths: string[]
}

interface FolderRollup {
  added: number
  removed: number
  modified: number
}

/** Estimated row heights, refined by measuring each row as it is drawn. */
const FOLDER_ROW_HEIGHT = 24
const FILE_ROW_HEIGHT = 26

/**
 * How many files under a folder were added, removed, or edited. Renames count
 * as modified: the file is still there, just under a new name, and splitting
 * them into their own bucket adds a word without adding meaning.
 */
function rollup(files: FileChange[]): FolderRollup {
  const counts: FolderRollup = { added: 0, removed: 0, modified: 0 }
  for (const file of files) {
    if (file.status === 'A') counts.added += 1
    else if (file.status === 'D') counts.removed += 1
    else counts.modified += 1
  }
  return counts
}

/**
 * Per-folder tally on the folder row. Only non-zero buckets are drawn, so a
 * folder of plain edits shows one number rather than two zeroes. The counts are
 * files, not lines -- the +/- pair on a file row already means lines, so these
 * use icons to avoid reading as the same thing.
 */
function FolderCounts({ counts }: { counts: FolderRollup }) {
  const parts: Array<{ code: StatusCode; n: number; word: string; Icon: typeof Plus }> = [
    { code: 'A', n: counts.added, word: 'added', Icon: Plus },
    { code: 'D', n: counts.removed, word: 'removed', Icon: Trash2 },
    { code: 'M', n: counts.modified, word: 'modified', Icon: Pencil },
  ]
  const shown = parts.filter((part) => part.n > 0)
  if (shown.length === 0) return null
  // The icons are shorthand a screen reader has no words for, so the group
  // carries the words and the glyphs themselves are hidden from it.
  const spoken = shown.map((part) => `${part.n} ${part.word}`).join(', ')
  return (
    <TooltipHint label={spoken}>
      <span
        className="flex flex-none items-center gap-1.5 font-mono text-2xs tabular-nums"
        aria-label={spoken}
      >
        {shown.map((part) => (
          <span
            key={part.code}
            aria-hidden
            className="flex items-center gap-0.5"
            style={{ color: statusColor(part.code) }}
          >
            {part.n}
            <part.Icon className="size-3" />
          </span>
        ))}
      </span>
    </TooltipHint>
  )
}

/**
 * Folder grouping without folder icons; nesting carries the path context.
 *
 * Virtualized: the visible tree is flattened into rows (`flattenVisible`) and
 * only the rows inside the scroll area are drawn. Every file row carries a
 * right-click menu and a tooltip button, and every click re-renders the rows
 * because the "something is staging" flag they read changes - with around a
 * thousand changed files, drawing them all froze the window while staging a
 * few.
 */
export function FileChangeTree({
  files,
  allFiles,
  treeId,
  staged,
  viewMode = 'tree',
  operationsDisabled,
  mutations: m,
  renderFile,
  scrollRef,
}: FileChangeTreeProps) {
  const root = useMemo(() => buildChangeTree(files), [files])
  // Worked out once per list rather than by scanning every file again for each
  // folder row as it is drawn.
  const groupFilesByFolder = useMemo(() => filesByFolder(files), [files])
  const discardFilesByFolder = useMemo(() => filesByFolder(allFiles), [allFiles])
  const allFolderKeys = useMemo(() => folderKeys(root, treeId), [root, treeId])
  const repo = useActiveRepo()
  const storeKey = repo ? changeTreeKey(repo.path, treeId) : null
  const saveExpanded = useWorkspaceStore((s) => s.setExpandedChangeFolders)
  const savedFolders = useWorkspaceStore(
    (s) => (storeKey ? s.expandedChangeFolders[storeKey] : undefined),
  )
  // Folder paths are stored bare so settings.json stays readable; the treeId
  // prefix that keys the rendered rows is re-applied here.
  const expanded = useMemo(
    () => new Set((savedFolders ?? []).map((path) => `${treeId}:${path}`)),
    [savedFolders, treeId],
  )
  const setExpanded = useCallback(
    (update: (current: Set<string>) => Set<string>) => {
      if (!storeKey) return
      const next = update(expanded)
      const prefix = `${treeId}:`
      saveExpanded(storeKey, [...next].map((key) => key.slice(prefix.length)))
    },
    [expanded, saveExpanded, storeKey, treeId],
  )
  const [discardFolder, setDiscardFolder] = useState<PendingFolderDiscard | null>(null)
  const folderOperationPending = m.stageFiles.isPending || m.unstageFiles.isPending || m.discardFiles.isPending
  const allExpanded = allFolderKeys.length > 0 && allFolderKeys.every((key) => expanded.has(key))

  const toggle = (path: string) => {
    setExpanded((current) => {
      const next = new Set(current)
      if (next.has(path)) next.delete(path)
      else next.add(path)
      return next
    })
  }

  const rows = useMemo(
    () => (viewMode === 'list' ? flatRows(files, treeId) : flattenVisible(root, expanded, treeId)),
    [viewMode, files, root, expanded, treeId],
  )

  // The list shares its scroll area with the bar above it, so the virtualizer
  // is told where the list starts inside that area.
  const listRef = useRef<HTMLDivElement>(null)
  const [scrollMargin, setScrollMargin] = useState(0)
  useLayoutEffect(() => {
    const top = listRef.current?.offsetTop ?? 0
    setScrollMargin((current) => (current === top ? current : top))
  })

  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: (index) => (rows[index]?.kind === 'folder' ? FOLDER_ROW_HEIGHT : FILE_ROW_HEIGHT),
    getItemKey: (index) => rows[index]?.key ?? index,
    overscan: 16,
    scrollMargin,
  })

  const renderFolder = (directory: ChangeTreeNode<FileChange>, depth: number, isExpanded: boolean) => {
    const key = `${treeId}:${directory.path}`
    const groupFiles = groupFilesByFolder.get(directory.path) ?? []
    const discardFiles = discardFilesByFolder.get(directory.path) ?? []
    const groupPaths = groupFiles.map((file) => file.path)
    const discardPaths = discardFiles.map((file) => file.path)
    const hasConflicts = groupFiles.some((file) => file.conflicted)
    const counts = rollup(groupFiles)
    const isFolderPending =
      (m.stageFiles.isPending && m.stageFiles.variables?.folder === directory.path) ||
      (m.unstageFiles.isPending && m.unstageFiles.variables?.folder === directory.path) ||
      (m.discardFiles.isPending && m.discardFiles.variables?.folder === directory.path)
    return (
      <ContextMenu>
        <ContextMenuTrigger asChild>
          <button
            type="button"
            role="treeitem"
            aria-level={depth + 1}
            aria-expanded={isExpanded}
            onClick={() => toggle(key)}
            style={{ paddingLeft: 10 + depth * 14 }}
            className={cn(
              'flex h-6 w-full items-center gap-1.5 pr-3.5 text-left text-xs font-medium text-sub hover:bg-panel2 hover:text-foreground',
              isFolderPending && 'bg-soft text-accent-text',
            )}
          >
            {isExpanded
              ? <ChevronDown aria-hidden className="size-3 flex-none" />
              : <ChevronRight aria-hidden className="size-3 flex-none" />}
            <span className="min-w-0 flex-1 overflow-hidden text-ellipsis whitespace-nowrap">{directory.name}</span>
            <FolderCounts counts={counts} />
            {isFolderPending && <PendingIndicator className="size-3 flex-none" />}
          </button>
        </ContextMenuTrigger>
        <ContextMenuContent className="w-60">
          <ContextMenuLabel className="overflow-hidden text-ellipsis whitespace-nowrap font-mono text-2xs text-sub">
            {directory.path} · {groupFiles.length} file{groupFiles.length === 1 ? '' : 's'}
          </ContextMenuLabel>
          <ContextMenuSeparator />
          {staged ? (
            <PendingMenuItem
              icon={<MinusCircle />}
              label="Unstage this folder"
              pendingLabel="Unstaging folder…"
              pending={m.unstageFiles.isPending && m.unstageFiles.variables?.folder === directory.path}
              disabled={operationsDisabled || folderOperationPending}
              onRun={() => m.unstageFiles.mutate({ folder: directory.path, paths: groupPaths })}
            />
          ) : (
            <PendingMenuItem
              icon={<PlusCircle />}
              label={hasConflicts ? 'Resolve conflicts first' : 'Stage this folder'}
              pendingLabel="Staging folder…"
              pending={m.stageFiles.isPending && m.stageFiles.variables?.folder === directory.path}
              disabled={hasConflicts || operationsDisabled || folderOperationPending}
              onRun={() => m.stageFiles.mutate({ folder: directory.path, paths: groupPaths })}
            />
          )}
          <PendingMenuItem
            icon={<Archive />}
            label="Stash this folder"
            pendingLabel="Stashing folder…"
            pending={m.stashFolder.isPending && m.stashFolder.variables?.folder === directory.path}
            disabled={operationsDisabled || m.stashFolder.isPending}
            onRun={() => m.stashFolder.mutate({ folder: directory.path })}
          />
          <IgnoreMenuItems
            path={directory.path}
            isFolder
            disabled={operationsDisabled}
          />
          <ContextMenuSeparator />
          <PendingMenuItem
            icon={<FolderOpen />}
            label="Open in Explorer"
            pendingLabel="Opening…"
            pending={m.openFolder.isPending && m.openFolder.variables === directory.path}
            onRun={() => m.openFolder.mutate(directory.path)}
          />
          <ContextMenuSeparator />
          <ContextMenuItem
            variant="destructive"
            disabled={operationsDisabled || folderOperationPending || discardPaths.length === 0}
            onSelect={() => setDiscardFolder({
              name: directory.name,
              path: directory.path,
              paths: discardPaths,
            })}
          >
            <Trash2 />
            Discard folder changes
          </ContextMenuItem>
        </ContextMenuContent>
      </ContextMenu>
    )
  }

  return (
    <>
      {viewMode === 'tree' && allFolderKeys.length > 0 && (
        <div className="flex items-center justify-end gap-1 border-b border-border/50 px-3.5 py-1">
          <button
            type="button"
            onClick={() => setExpanded(() => allExpanded ? new Set() : new Set(allFolderKeys))}
            className="flex items-center gap-1 rounded px-1.5 py-0.5 text-2xs text-sub hover:bg-panel2 hover:text-foreground"
          >
            {allExpanded
              ? <ChevronsDownUp aria-hidden className="size-3 flex-none" />
              : <ChevronsUpDown aria-hidden className="size-3 flex-none" />}
            {allExpanded ? 'Collapse all' : 'Expand all'}
          </button>
        </div>
      )}
      <div
        ref={listRef}
        role={viewMode === 'list' ? 'list' : 'tree'}
        aria-label={staged ? 'Staged changed files' : 'Unstaged changed files'}
        style={{ height: virtualizer.getTotalSize(), position: 'relative' }}
      >
        {virtualizer.getVirtualItems().map((item) => {
          const row = rows[item.index]
          if (!row) return null
          return (
            <div
              key={item.key}
              data-index={item.index}
              ref={virtualizer.measureElement}
              role={viewMode === 'list' ? 'listitem' : 'none'}
              className="absolute left-0 top-0 w-full"
              style={{ transform: `translateY(${item.start - scrollMargin}px)` }}
            >
              {row.kind === 'folder'
                ? renderFolder(row.node, row.depth, row.expanded)
                : renderFile(row.file, row.name, row.depth)}
            </div>
          )
        })}
      </div>
      <ConfirmDialog
        open={discardFolder != null}
        onOpenChange={(open) => !open && setDiscardFolder(null)}
        destructive
        title={`Discard all changes in ${discardFolder?.name ?? 'this folder'}?`}
        description={
          <>
            This throws away staged and unstaged changes in{' '}
            <span className="font-mono text-foreground">{discardFolder?.path}</span> across{' '}
            <span className="text-foreground">{discardFolder?.paths.length ?? 0}</span> file
            {(discardFolder?.paths.length ?? 0) === 1 ? '' : 's'}. This can't be undone.
          </>
        }
        confirmLabel="Discard folder changes"
        pending={m.discardFiles.isPending}
        pendingLabel="Discarding folder changes…"
        keepOpenOnConfirm
        onConfirm={() => discardFolder && m.discardFiles.mutate(
          { folder: discardFolder.path, paths: discardFolder.paths },
          { onSuccess: () => setDiscardFolder(null) },
        )}
      />
    </>
  )
}
