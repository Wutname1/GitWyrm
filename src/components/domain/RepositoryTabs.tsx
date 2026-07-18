import { Fragment, useRef, useState, type CSSProperties, type DragEvent } from 'react'
import {
  Check,
  ChevronRight,
  Layers3,
  Pencil,
  Save,
  Trash2,
  Ungroup,
  X,
} from 'lucide-react'
import { toast } from 'sonner'
import type { RepoInfo } from '@/lib/bindings'
import { commands } from '@/lib/bindings'
import { cn } from '@/lib/utils'
import { normalizePath } from '@/lib/paths'
import {
  TAB_GROUP_COLORS,
  useWorkspaceStore,
  type TabDropPlacement,
  type TabGroup,
  type TabOrderItem,
} from '@/stores/workspaceStore'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuLabel,
  ContextMenuSeparator,
  ContextMenuSub,
  ContextMenuSubContent,
  ContextMenuSubTrigger,
  ContextMenuTrigger,
} from '@/components/ui/context-menu'
import { TooltipButton } from '@/components/ui/tooltip'

export type TabOrientation = 'horizontal' | 'vertical'

type DragItem =
  | { type: 'repo'; path: string }
  | { type: 'group'; id: string }

type DropTarget =
  | { type: 'order'; index: number }
  | { type: 'repo'; path: string; placement: TabDropPlacement | 'group' }
  | { type: 'group'; id: string }

interface RenameTarget {
  type: 'tab' | 'group'
  id: string
  value: string
  fallback?: string
}

function pathKey(path: string): string {
  return normalizePath(path).toLowerCase()
}

function samePath(left: string, right: string): boolean {
  return pathKey(left) === pathKey(right)
}

function RenameDialog({
  target,
  onClose,
  onSave,
}: {
  target: RenameTarget
  onClose: () => void
  onSave: (value: string) => void
}) {
  const [value, setValue] = useState(target.value)
  const title = target.type === 'group' ? 'Rename group' : 'Rename tab'
  const label = target.type === 'group' ? 'Group name' : 'Tab name'

  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="gap-0 p-0 sm:max-w-md" aria-describedby={undefined}>
        <DialogHeader className="border-b border-border px-4 pb-3 pt-4">
          <DialogTitle className="flex items-center gap-2 text-sm">
            <Pencil size={15} strokeWidth={1.9} />
            {title}
          </DialogTitle>
        </DialogHeader>
        <form
          onSubmit={(event) => {
            event.preventDefault()
            onSave(value)
          }}
        >
          <div className="grid gap-1.5 px-4 py-4">
            <label className="text-[11px] font-semibold text-sub">{label}</label>
            <Input
              value={value}
              onChange={(event) => setValue(event.target.value)}
              placeholder={target.fallback}
              className="h-auto bg-background py-1.5 text-xs"
              autoFocus
            />
            {target.type === 'tab' && (
              <p className="text-[10.5px] text-muted-foreground">
                Leave blank to use the folder name.
              </p>
            )}
          </div>
          <DialogFooter className="flex justify-end gap-2 border-t border-border px-4 py-3">
            <Button type="button" variant="secondary" size="sm" onClick={onClose}>
              Cancel
            </Button>
            <Button type="submit" size="sm" disabled={target.type === 'group' && !value.trim()}>
              Save
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}

function groupStyle(color: string): CSSProperties {
  return { '--tab-group-color': color } as CSSProperties
}

function DropGap({ orientation, active, label }: {
  orientation: TabOrientation
  active: boolean
  label: string
}) {
  return (
    <div
      aria-hidden="true"
      className={cn(
        'gw-tab-drop-gap grid flex-none place-items-center overflow-hidden rounded-[5px] border-dashed text-[9px] font-semibold text-primary transition-[width,height,margin,border-color,background-color] duration-150',
        orientation === 'horizontal'
          ? active ? 'mx-0.5 h-full w-24 border border-primary/60 bg-soft' : 'h-full w-0 border-0'
          : active ? 'my-0.5 h-8 w-full border border-primary/60 bg-soft' : 'h-0 w-full border-0',
      )}
    >
      {active ? label : ''}
    </div>
  )
}

function findRepo(openRepos: RepoInfo[], path: string): RepoInfo | undefined {
  return openRepos.find((repo) => samePath(repo.path, path))
}

function orderedPaths(order: TabOrderItem[], groups: TabGroup[]): string[] {
  return order.flatMap((item) => item.type === 'repo'
    ? [item.path]
    : groups.find((group) => group.id === item.id)?.repoPaths ?? [])
}

export function RepositoryTabs({ orientation }: { orientation: TabOrientation }) {
  const openRepos = useWorkspaceStore((state) => state.openRepos)
  const activeRepoId = useWorkspaceStore((state) => state.activeRepoId)
  const tabAliases = useWorkspaceStore((state) => state.tabAliases)
  const tabGroups = useWorkspaceStore((state) => state.tabGroups)
  const tabOrder = useWorkspaceStore((state) => state.tabOrder)
  const savedTabGroups = useWorkspaceStore((state) => state.savedTabGroups)
  const [renaming, setRenaming] = useState<RenameTarget | null>(null)
  const [dragItem, setDragItem] = useState<DragItem | null>(null)
  const [dropTarget, setDropTarget] = useState<DropTarget | null>(null)
  const draggedGroupRef = useRef<string | null>(null)

  const repoName = (repo: RepoInfo) => tabAliases[repo.path] ?? repo.name
  const isSaved = (groupId: string) => savedTabGroups.some((group) => group.id === groupId)

  const setTarget = (next: DropTarget | null) => {
    setDropTarget((current) => JSON.stringify(current) === JSON.stringify(next) ? current : next)
  }

  const closeRepo = (repo: RepoInfo) => {
    void commands.closeRepo(repo.id)
    useWorkspaceStore.getState().removeRepo(repo.id)
    toast.success(`Closed ${repoName(repo)}`)
  }

  const closeGroup = (group: TabGroup) => {
    for (const path of group.repoPaths) {
      const repo = findRepo(openRepos, path)
      if (repo) void commands.closeRepo(repo.id)
    }
    useWorkspaceStore.getState().removeTabGroup(group.id)
    toast.success(`Closed ${group.name} and ${group.repoPaths.length} repositories`)
  }

  const createGroup = (paths: string[]) => {
    const id = useWorkspaceStore.getState().createTabGroup(paths)
    setRenaming({ type: 'group', id, value: 'New group' })
    toast.success(`Created a group with ${paths.length} ${paths.length === 1 ? 'repository' : 'repositories'}`)
  }

  const finishDrag = () => {
    setDragItem(null)
    setTarget(null)
  }

  const startRepoDrag = (event: DragEvent<HTMLElement>, repo: RepoInfo) => {
    event.stopPropagation()
    event.dataTransfer.effectAllowed = 'move'
    event.dataTransfer.setData('text/plain', repo.path)
    setDragItem({ type: 'repo', path: repo.path })
    setTarget(null)
    toast.info(`Moving ${repoName(repo)}. Use an edge to reorder or the center to group.`)
  }

  const startGroupDrag = (event: DragEvent<HTMLElement>, group: TabGroup) => {
    event.stopPropagation()
    event.dataTransfer.effectAllowed = 'move'
    event.dataTransfer.setData('text/plain', `group:${group.id}`)
    draggedGroupRef.current = group.id
    setDragItem({ type: 'group', id: group.id })
    setTarget(null)
    toast.info(`Moving ${group.name} with all ${group.repoPaths.length} repositories`)
  }

  const dropOnOrder = (index: number) => {
    if (!dragItem) return
    if (dragItem.type === 'group') {
      useWorkspaceStore.getState().moveGroupToOrder(dragItem.id, index)
      const group = tabGroups.find((candidate) => candidate.id === dragItem.id)
      toast.success(`${group?.name ?? 'Group'} moved`)
    } else {
      useWorkspaceStore.getState().moveRepoToOrder(dragItem.path, index)
      const repo = findRepo(openRepos, dragItem.path)
      toast.success(`${repo ? repoName(repo) : 'Repository'} moved between tabs`)
    }
    finishDrag()
  }

  const dropOnRepo = (targetPath: string, placement: TabDropPlacement | 'group') => {
    if (dragItem?.type !== 'repo' || samePath(dragItem.path, targetPath)) return
    const store = useWorkspaceStore.getState()
    const sourceRepo = findRepo(openRepos, dragItem.path)
    const targetRepo = findRepo(openRepos, targetPath)
    const targetGroup = tabGroups.find((group) => group.repoPaths.some((path) => samePath(path, targetPath)))
    const sourceGroup = tabGroups.find((group) => group.repoPaths.some((path) => samePath(path, dragItem.path)))

    if (placement !== 'group') {
      store.moveRepoBeside(dragItem.path, targetPath, placement)
      toast.success(`${sourceRepo ? repoName(sourceRepo) : 'Repository'} moved ${placement} ${targetRepo ? repoName(targetRepo) : 'the tab'}`)
    } else if (targetGroup) {
      if (sourceGroup?.id === targetGroup.id) {
        toast.info(`${sourceRepo ? repoName(sourceRepo) : 'Repository'} is already in ${targetGroup.name}`)
      } else {
        store.addRepoToGroup(dragItem.path, targetGroup.id)
        toast.success(`${sourceRepo ? repoName(sourceRepo) : 'Repository'} added to ${targetGroup.name}`)
      }
    } else {
      createGroup([targetPath, dragItem.path])
    }
    finishDrag()
  }

  const dropOnGroup = (group: TabGroup) => {
    if (dragItem?.type !== 'repo') return
    const currentGroup = tabGroups.find((candidate) => candidate.repoPaths.some((path) => samePath(path, dragItem.path)))
    const repo = findRepo(openRepos, dragItem.path)
    if (currentGroup?.id === group.id) {
      toast.info(`${repo ? repoName(repo) : 'Repository'} is already in ${group.name}`)
    } else {
      useWorkspaceStore.getState().addRepoToGroup(dragItem.path, group.id)
      toast.success(`${repo ? repoName(repo) : 'Repository'} added to ${group.name}`)
    }
    finishDrag()
  }

  const closeOthers = (keepPath: string) => {
    for (const repo of openRepos) {
      if (!samePath(repo.path, keepPath)) {
        void commands.closeRepo(repo.id)
        useWorkspaceStore.getState().removeRepo(repo.id)
      }
    }
    toast.success('Closed the other repositories')
  }

  const closeAfter = (path: string) => {
    const paths = orderedPaths(tabOrder, tabGroups)
    const index = paths.findIndex((candidate) => samePath(candidate, path))
    if (index < 0) return
    const closing = paths.slice(index + 1)
    for (const repoPath of closing) {
      const repo = findRepo(openRepos, repoPath)
      if (repo) {
        void commands.closeRepo(repo.id)
        useWorkspaceStore.getState().removeRepo(repo.id)
      }
    }
    toast.success(`Closed ${closing.length} ${closing.length === 1 ? 'repository' : 'repositories'}`)
  }

  const renderRepoTab = (repo: RepoInfo, group: TabGroup | null) => {
    const target = dropTarget?.type === 'repo' && samePath(dropTarget.path, repo.path)
      ? dropTarget.placement
      : null
    const inGroup = group != null
    const active = repo.id === activeRepoId
    const groupsForMenu = tabGroups.filter((candidate) => candidate.id !== group?.id)
    const pathOrder = orderedPaths(tabOrder, tabGroups)
    const pathIndex = pathOrder.findIndex((path) => samePath(path, repo.path))

    return (
      <div
        key={repo.path}
        className={cn('flex flex-none', orientation === 'horizontal' ? 'h-full flex-row' : 'w-full flex-col')}
        onDragOver={(event) => {
          if (dragItem?.type !== 'repo' || samePath(dragItem.path, repo.path)) return
          event.preventDefault()
          event.stopPropagation()
          const tab = event.currentTarget.querySelector<HTMLElement>('[data-repo-tab]')
          if (!tab) return
          const rect = tab.getBoundingClientRect()
          const pointer = orientation === 'horizontal' ? event.clientX - rect.left : event.clientY - rect.top
          const ratio = pointer / (orientation === 'horizontal' ? rect.width : rect.height)
          setTarget({
            type: 'repo',
            path: repo.path,
            placement: ratio < 0.3 ? 'before' : ratio > 0.7 ? 'after' : 'group',
          })
        }}
        onDragLeave={(event) => {
          if (!event.currentTarget.contains(event.relatedTarget as Node | null) && target) setTarget(null)
        }}
        onDrop={(event) => {
          event.preventDefault()
          event.stopPropagation()
          dropOnRepo(repo.path, target ?? 'group')
        }}
      >
        <DropGap orientation={orientation} active={target === 'before'} label="Move here" />
        <ContextMenu>
          <ContextMenuTrigger asChild>
            <div
              data-repo-tab
              draggable
              onDragStart={(event) => startRepoDrag(event, repo)}
              onDragEnd={finishDrag}
              onClick={() => useWorkspaceStore.getState().setActiveRepo(repo.id)}
              className={cn(
                'group/repo relative flex flex-none cursor-pointer items-center gap-[7px] text-xs transition-[border-color,background-color,color]',
                orientation === 'horizontal'
                  ? 'h-full min-w-24 max-w-40 border-l px-2.5'
                  : 'h-[31px] w-full rounded-[5px] border px-2 pl-5',
                inGroup && orientation === 'horizontal' ? 'border-[color:color-mix(in_srgb,var(--tab-group-color)_20%,var(--gw-border))]' : 'border-border',
                active
                  ? inGroup
                    ? 'bg-[color:color-mix(in_srgb,var(--tab-group-color)_10%,var(--gw-panel))] font-semibold text-foreground'
                    : 'bg-panel font-semibold text-foreground'
                  : 'text-sub hover:bg-panel2 hover:text-foreground',
                target === 'group' && 'z-10 border-primary! bg-soft! shadow-[0_0_0_2px_rgba(45,212,167,.13)]',
              )}
              style={group ? groupStyle(group.color) : undefined}
              title={repo.path}
            >
              <span
                className="size-[7px] flex-none rounded-[2px]"
                style={{ background: group?.color ?? 'var(--gw-accent)' }}
              />
              <span className="min-w-0 flex-1 overflow-hidden text-ellipsis whitespace-nowrap">
                {repoName(repo)}
              </span>
              <TooltipButton
                draggable={false}
                onClick={(event) => {
                  event.stopPropagation()
                  closeRepo(repo)
                }}
                className="ml-auto flex-none rounded p-0.5 text-muted-foreground opacity-0 hover:bg-panel3 hover:text-foreground group-hover/repo:opacity-100"
                tooltip="Close repository"
              >
                <X size={11} />
              </TooltipButton>
            </div>
          </ContextMenuTrigger>
          <ContextMenuContent className="w-52">
            <ContextMenuItem onSelect={() => setRenaming({
              type: 'tab',
              id: repo.path,
              value: tabAliases[repo.path] ?? '',
              fallback: repo.name,
            })}>
              <Pencil size={13} strokeWidth={2} />
              Rename tab
            </ContextMenuItem>
            <ContextMenuItem onSelect={() => createGroup([repo.path])}>
              <Layers3 size={13} strokeWidth={2} />
              Create new group
            </ContextMenuItem>
            {groupsForMenu.length > 0 && (
              <ContextMenuSub>
                <ContextMenuSubTrigger>
                  <Layers3 size={13} strokeWidth={2} />
                  Add to group
                </ContextMenuSubTrigger>
                <ContextMenuSubContent className="w-44">
                  {groupsForMenu.map((candidate) => (
                    <ContextMenuItem
                      key={candidate.id}
                      onSelect={() => {
                        useWorkspaceStore.getState().addRepoToGroup(repo.path, candidate.id)
                        toast.success(`${repoName(repo)} added to ${candidate.name}`)
                      }}
                    >
                      <span className="size-2 rounded-full" style={{ background: candidate.color }} />
                      {candidate.name}
                    </ContextMenuItem>
                  ))}
                </ContextMenuSubContent>
              </ContextMenuSub>
            )}
            {group && (
              <ContextMenuItem onSelect={() => {
                useWorkspaceStore.getState().removeRepoFromGroup(repo.path)
                toast.success(`${repoName(repo)} removed from ${group.name}`)
              }}>
                <Ungroup size={13} strokeWidth={2} />
                Remove from group
              </ContextMenuItem>
            )}
            <ContextMenuSeparator />
            <ContextMenuItem onSelect={() => closeRepo(repo)}>
              <X size={13} strokeWidth={2} />
              Close tab
            </ContextMenuItem>
            <ContextMenuItem disabled={openRepos.length <= 1} onSelect={() => closeOthers(repo.path)}>
              Close other tabs
            </ContextMenuItem>
            <ContextMenuItem
              disabled={pathIndex < 0 || pathIndex === pathOrder.length - 1}
              onSelect={() => closeAfter(repo.path)}
            >
              Close tabs after this
            </ContextMenuItem>
          </ContextMenuContent>
        </ContextMenu>
        <DropGap orientation={orientation} active={target === 'after'} label="Move here" />
      </div>
    )
  }

  const renderGroup = (group: TabGroup) => {
    const groupTarget = dropTarget?.type === 'group' && dropTarget.id === group.id
    const saved = isSaved(group.id)
    return (
      <ContextMenu key={group.id}>
        <ContextMenuTrigger asChild>
          <section
            data-tab-group={group.id}
            className={cn(
              'gw-tab-group flex flex-none border-[color:var(--tab-group-color)] transition-opacity',
              orientation === 'horizontal'
                ? 'h-full flex-row border-b-2 bg-[color:color-mix(in_srgb,var(--tab-group-color)_5%,transparent)]'
                : 'relative w-full flex-col border-l-2 pl-[3px]',
              dragItem?.type === 'group' && dragItem.id === group.id && 'opacity-35',
            )}
            style={groupStyle(group.color)}
          >
            <button
              type="button"
              draggable
              onDragStart={(event) => startGroupDrag(event, group)}
              onDragEnd={() => {
                draggedGroupRef.current = group.id
                window.setTimeout(() => {
                  if (draggedGroupRef.current === group.id) draggedGroupRef.current = null
                }, 200)
                finishDrag()
              }}
              onDragOver={(event) => {
                if (dragItem?.type !== 'repo') return
                event.preventDefault()
                event.stopPropagation()
                setTarget({ type: 'group', id: group.id })
              }}
              onDragLeave={(event) => {
                if (!event.currentTarget.contains(event.relatedTarget as Node | null) && groupTarget) setTarget(null)
              }}
              onDrop={(event) => {
                event.preventDefault()
                event.stopPropagation()
                dropOnGroup(group)
              }}
              onClick={() => {
                if (draggedGroupRef.current === group.id) {
                  draggedGroupRef.current = null
                  return
                }
                useWorkspaceStore.getState().toggleTabGroup(group.id)
                toast.info(`${group.name} ${group.collapsed ? 'expanded' : 'collapsed'}`)
              }}
              className={cn(
                'flex flex-none cursor-grab items-center gap-1.5 text-left font-semibold outline-none active:cursor-grabbing',
                orientation === 'horizontal'
                  ? 'h-full min-w-8 px-2 text-[10.5px]'
                  : 'h-[29px] w-full rounded-[5px] px-1.5 text-[11px] hover:bg-panel2',
                groupTarget && 'bg-soft shadow-[inset_0_0_0_1px_var(--gw-accent)]',
              )}
              style={{ color: group.color }}
              title={`Click to ${group.collapsed ? 'expand' : 'collapse'}. Drag to move the group.`}
            >
              <ChevronRight
                size={11}
                strokeWidth={2.2}
                className={cn('flex-none transition-transform', !group.collapsed && 'rotate-90')}
              />
              <span className="max-w-28 overflow-hidden text-ellipsis whitespace-nowrap text-foreground">
                {group.name}
              </span>
              {saved && <Save size={10} strokeWidth={2} aria-label="Saved group" />}
              <span className="font-mono text-[9px] opacity-65">{group.repoPaths.length}</span>
            </button>
            {!group.collapsed && (
              <div className={cn('flex', orientation === 'horizontal' ? 'h-full flex-row' : 'w-full flex-col gap-0.5')}>
                {group.repoPaths.map((path) => {
                  const repo = findRepo(openRepos, path)
                  return repo ? renderRepoTab(repo, group) : null
                })}
              </div>
            )}
          </section>
        </ContextMenuTrigger>
        <ContextMenuContent className="w-52">
          <ContextMenuLabel className="text-[10px] tracking-wide text-muted-foreground">
            {group.name.toUpperCase()} · {group.repoPaths.length} REPOS
          </ContextMenuLabel>
          <ContextMenuItem onSelect={() => {
            useWorkspaceStore.getState().toggleTabGroup(group.id)
            toast.info(`${group.name} ${group.collapsed ? 'expanded' : 'collapsed'}`)
          }}>
            <ChevronRight size={13} className={cn(!group.collapsed && 'rotate-90')} />
            {group.collapsed ? 'Expand group' : 'Collapse group'}
          </ContextMenuItem>
          <ContextMenuItem onSelect={() => setRenaming({ type: 'group', id: group.id, value: group.name })}>
            <Pencil size={13} strokeWidth={2} />
            Rename group
          </ContextMenuItem>
          <ContextMenuSub>
            <ContextMenuSubTrigger>
              <span className="size-2.5 rounded-full" style={{ background: group.color }} />
              Change color
            </ContextMenuSubTrigger>
            <ContextMenuSubContent className="w-40">
              {TAB_GROUP_COLORS.map((color) => (
                <ContextMenuItem
                  key={color}
                  onSelect={() => {
                    useWorkspaceStore.getState().setTabGroupColor(group.id, color)
                    toast.success(`${group.name} color changed`)
                  }}
                >
                  <span className="size-3 rounded-full" style={{ background: color }} />
                  <span className="flex-1">{color.toUpperCase()}</span>
                  {group.color === color && <Check size={12} />}
                </ContextMenuItem>
              ))}
            </ContextMenuSubContent>
          </ContextMenuSub>
          <ContextMenuItem onSelect={() => {
            useWorkspaceStore.getState().saveTabGroup(group.id)
            toast.success(`${group.name} saved for later`)
          }}>
            <Save size={13} strokeWidth={2} />
            {saved ? 'Update saved group' : 'Save group'}
          </ContextMenuItem>
          <ContextMenuSeparator />
          <ContextMenuItem onSelect={() => {
            useWorkspaceStore.getState().ungroupTabGroup(group.id)
            toast.success(`${group.name} ungrouped. Its repositories stayed in place.`)
          }}>
            <Ungroup size={13} strokeWidth={2} />
            Ungroup
          </ContextMenuItem>
          <ContextMenuItem variant="destructive" onSelect={() => closeGroup(group)}>
            <Trash2 size={13} strokeWidth={2} />
            Close group
          </ContextMenuItem>
        </ContextMenuContent>
      </ContextMenu>
    )
  }

  const renderOrderGap = (index: number) => {
    const active = dropTarget?.type === 'order' && dropTarget.index === index
    return (
      <div
        key={`gap-${index}`}
        data-tab-order-gap={index}
        onDragOver={(event) => {
          if (!dragItem) return
          event.preventDefault()
          event.stopPropagation()
          setTarget({ type: 'order', index })
        }}
        onDragLeave={(event) => {
          if (!event.currentTarget.contains(event.relatedTarget as Node | null) && active) setTarget(null)
        }}
        onDrop={(event) => {
          event.preventDefault()
          event.stopPropagation()
          dropOnOrder(index)
        }}
        className={cn(
          'grid flex-none place-items-center overflow-hidden rounded-[5px] border-dashed text-[9px] font-semibold text-primary transition-[width,height,margin,border-color,background-color] duration-150',
          orientation === 'horizontal'
            ? active
              ? 'mx-0.5 h-full w-24 border border-primary/60 bg-soft'
              : dragItem ? 'h-full w-1.5' : 'h-full w-0.5'
            : active
              ? 'my-0.5 h-8 w-full border border-primary/60 bg-soft'
              : dragItem ? 'h-1.5 w-full' : 'h-0.5 w-full',
        )}
      >
        {active ? dragItem?.type === 'group' ? 'Move group here' : 'Move here' : ''}
      </div>
    )
  }

  return (
    <>
      <div
        data-dim-on-drag
        className={cn(
          'gw-repository-tabs flex min-h-0 min-w-0',
          orientation === 'horizontal'
            ? 'gw-tab-scroll h-full flex-1 flex-row items-stretch overflow-x-auto overflow-y-hidden'
            : 'w-full flex-1 flex-col overflow-y-auto overflow-x-hidden px-1.5 py-1',
        )}
        onDragEnd={finishDrag}
      >
        {tabOrder.map((item, index) => (
          <Fragment key={item.type === 'group' ? `group-${item.id}` : `repo-${pathKey(item.path)}`}>
            {renderOrderGap(index)}
            {item.type === 'group'
              ? (() => {
                  const group = tabGroups.find((candidate) => candidate.id === item.id)
                  return group ? renderGroup(group) : null
                })()
              : (() => {
                  const repo = findRepo(openRepos, item.path)
                  return repo ? renderRepoTab(repo, null) : null
                })()}
          </Fragment>
        ))}
        {renderOrderGap(tabOrder.length)}
      </div>

      {renaming && (
        <RenameDialog
          key={`${renaming.type}-${renaming.id}`}
          target={renaming}
          onClose={() => setRenaming(null)}
          onSave={(value) => {
            if (renaming.type === 'tab') {
              useWorkspaceStore.getState().setTabAlias(renaming.id, value)
              const repo = findRepo(openRepos, renaming.id)
              toast.success(value.trim() ? `Tab renamed to ${value.trim()}` : `Tab name reset to ${repo?.name ?? 'folder name'}`)
            } else {
              useWorkspaceStore.getState().renameTabGroup(renaming.id, value)
              toast.success(`Group renamed to ${value.trim()}`)
            }
            setRenaming(null)
          }}
        />
      )}
    </>
  )
}
