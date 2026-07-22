import { useMemo, useState, type ReactNode } from 'react'
import { ChevronDown, ChevronRight, MinusCircle, PlusCircle, Trash2 } from 'lucide-react'
import type { FileChange } from '@/lib/bindings'
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuLabel,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from '@/components/ui/context-menu'
import { PendingMenuItem } from '@/components/ui/pending-menu-item'
import { PendingIndicator } from '@/components/ui/pending-indicator'
import { ConfirmDialog } from '@/components/modals/ConfirmDialog'
import { IgnoreMenuItems } from '@/components/domain/commit-form/IgnoreMenuItems'
import { useGitMutations } from '@/hooks/useGitMutations'
import { cn } from '@/lib/utils'

interface TreeNode {
  name: string
  path: string
  directories: Map<string, TreeNode>
  files: Array<{ name: string; file: FileChange }>
}

function buildTree(files: FileChange[]): TreeNode {
  const root: TreeNode = { name: '', path: '', directories: new Map(), files: [] }

  for (const file of files) {
    const segments = file.path.replaceAll('\\', '/').split('/').filter(Boolean)
    const fileName = segments.pop() ?? file.path
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
    node.files.push({ name: fileName, file })
  }

  return root
}

function folderKeys(node: TreeNode, treeId: string): string[] {
  const keys: string[] = []
  for (const directory of node.directories.values()) {
    keys.push(`${treeId}:${directory.path}`, ...folderKeys(directory, treeId))
  }
  return keys
}

interface FileChangeTreeProps {
  files: FileChange[]
  /** All staged and unstaged files, used by recursive folder discard. */
  allFiles: FileChange[]
  treeId: string
  staged: boolean
  operationsDisabled?: boolean
  mutations: Pick<
    ReturnType<typeof useGitMutations>,
    'stageFiles' | 'unstageFiles' | 'discardFiles'
  >
  renderFile: (file: FileChange, name: string, depth: number) => ReactNode
}

interface PendingFolderDiscard {
  name: string
  path: string
  paths: string[]
}

function filesInFolder(files: FileChange[], folder: string): FileChange[] {
  const prefix = `${folder.replaceAll('\\', '/').replace(/\/$/, '')}/`
  const unique = new Map<string, FileChange>()
  for (const file of files) {
    const normalized = file.path.replaceAll('\\', '/')
    if (normalized.startsWith(prefix)) unique.set(normalized, file)
  }
  return [...unique.values()]
}

/** Folder grouping without folder icons; nesting carries the path context. */
export function FileChangeTree({
  files,
  allFiles,
  treeId,
  staged,
  operationsDisabled,
  mutations: m,
  renderFile,
}: FileChangeTreeProps) {
  const root = useMemo(() => buildTree(files), [files])
  const allFolderKeys = useMemo(() => folderKeys(root, treeId), [root, treeId])
  const [expanded, setExpanded] = useState<Set<string>>(() => new Set())
  const [discardFolder, setDiscardFolder] = useState<PendingFolderDiscard | null>(null)
  const folderOperationPending = m.stageFiles.isPending || m.unstageFiles.isPending || m.discardFiles.isPending
  const allExpanded = allFolderKeys.length > 0 && allFolderKeys.every((key) => expanded.has(key))
  const allCollapsed = allFolderKeys.every((key) => !expanded.has(key))

  const toggle = (path: string) => {
    setExpanded((current) => {
      const next = new Set(current)
      if (next.has(path)) next.delete(path)
      else next.add(path)
      return next
    })
  }

  const renderNode = (node: TreeNode, depth: number): ReactNode => {
    const directories = [...node.directories.values()].sort((left, right) => left.name.localeCompare(right.name))
    const fileNodes = [...node.files].sort((left, right) => left.name.localeCompare(right.name))

    return (
      <>
        {directories.map((directory) => {
          const key = `${treeId}:${directory.path}`
          const isExpanded = expanded.has(key)
          const groupFiles = filesInFolder(files, directory.path)
          const discardFiles = filesInFolder(allFiles, directory.path)
          const groupPaths = groupFiles.map((file) => file.path)
          const discardPaths = discardFiles.map((file) => file.path)
          const hasConflicts = groupFiles.some((file) => file.conflicted)
          const isFolderPending =
            (m.stageFiles.isPending && m.stageFiles.variables?.folder === directory.path) ||
            (m.unstageFiles.isPending && m.unstageFiles.variables?.folder === directory.path) ||
            (m.discardFiles.isPending && m.discardFiles.variables?.folder === directory.path)
          return (
            <div key={key} role="none">
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
                  <IgnoreMenuItems
                    path={directory.path}
                    isFolder
                    disabled={operationsDisabled}
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
              {isExpanded && (
                <div role="group">
                  {renderNode(directory, depth + 1)}
                </div>
              )}
            </div>
          )
        })}
        {fileNodes.map(({ name, file }) => (
          <div key={file.path} role="none">{renderFile(file, name, depth)}</div>
        ))}
      </>
    )
  }

  return (
    <>
      {allFolderKeys.length > 0 && (
        <div className="flex items-center justify-end gap-1 border-b border-border/50 px-3.5 py-1">
          <button
            type="button"
            onClick={() => setExpanded(new Set(allFolderKeys))}
            disabled={allExpanded}
            className="rounded px-1.5 py-0.5 text-2xs text-sub hover:bg-panel2 hover:text-foreground disabled:cursor-default disabled:opacity-40"
          >
            Expand all
          </button>
          <span aria-hidden className="text-muted-foreground">·</span>
          <button
            type="button"
            onClick={() => setExpanded(new Set())}
            disabled={allCollapsed}
            className="rounded px-1.5 py-0.5 text-2xs text-sub hover:bg-panel2 hover:text-foreground disabled:cursor-default disabled:opacity-40"
          >
            Collapse all
          </button>
        </div>
      )}
      <div role="tree" aria-label={staged ? 'Staged changed files' : 'Unstaged changed files'}>
        {renderNode(root, 0)}
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
        confirmPhrase="discard"
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
