import { useEffect, useMemo, useState } from 'react'
import { Clock, Download, Folder, FolderGit2, FolderSearch, Layers3, Loader2, RefreshCw, Trash2 } from 'lucide-react'
import { toast } from 'sonner'
import { listen } from '@tauri-apps/api/event'
import { cn } from '@/lib/utils'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { TooltipButton } from '@/components/ui/tooltip'
import { commands } from '@/lib/bindings'
import { joinPath, normalizePath } from '@/lib/paths'
import { unwrap } from '@/lib/queryKeys'
import { useCodeFolderRepos, useOpenRepo } from '@/hooks/useRepoActions'
import { useUiStore } from '@/stores/uiStore'
import { useWorkspaceStore } from '@/stores/workspaceStore'

interface GitProgressPayload {
  repo_id: string
  operation: string
  line: string
}

function RepoRow({
  icon,
  name,
  detail,
  meta,
  onClick,
  disabled,
}: {
  icon: React.ReactNode
  name: string
  detail?: string
  meta?: string
  onClick: () => void
  disabled?: boolean
}) {
  return (
    <button
      onClick={onClick}
      disabled={disabled}
      className="flex w-full items-center gap-2.5 rounded-md px-2.5 py-[7px] text-left hover:bg-panel3 disabled:opacity-50"
    >
      <span className="flex-none text-sub">{icon}</span>
      <span className="min-w-0 flex-1">
        <span className="block overflow-hidden text-ellipsis whitespace-nowrap text-xs font-medium text-foreground">
          {name}
        </span>
        {detail && (
          <span className="block overflow-hidden text-ellipsis whitespace-nowrap font-mono text-[9.5px] text-muted-foreground">
            {detail}
          </span>
        )}
      </span>
      {meta && <span className="flex-none font-mono text-[9.5px] text-primary">{meta}</span>}
    </button>
  )
}

function SectionLabel({ children }: { children: React.ReactNode }) {
  return (
    <div className="px-2.5 pb-1 pt-3 text-[9.5px] font-bold tracking-[.09em] text-muted-foreground">
      {children}
    </div>
  )
}

type Tab = 'open' | 'clone' | 'groups'

export function RepoPickerModal() {
  const open = useUiStore((s) => s.activeModal === 'clone')
  const closeModal = useUiStore((s) => s.closeModal)
  const recents = useWorkspaceStore((s) => s.recents)
  const openRepos = useWorkspaceStore((s) => s.openRepos)
  const openRepoIds = useMemo(
    () => new Set(openRepos.map((r) => r.path.toLowerCase())),
    [openRepos]
  )
  const codeFolder = useWorkspaceStore((s) => s.codeFolder)
  const setCodeFolder = useWorkspaceStore((s) => s.setCodeFolder)
  const cloneDirectory = useWorkspaceStore((s) => s.cloneDirectory)
  const setCloneDirectory = useWorkspaceStore((s) => s.setCloneDirectory)
  const addRepo = useWorkspaceStore((s) => s.addRepo)
  const savedTabGroups = useWorkspaceStore((s) => s.savedTabGroups)
  const deleteSavedTabGroup = useWorkspaceStore((s) => s.deleteSavedTabGroup)

  const scanned = useCodeFolderRepos()
  const openRepo = useOpenRepo()

  const [tab, setTab] = useState<Tab>('open')
  const [filter, setFilter] = useState('')

  // Clone state. Destination defaults to the code folder, then the saved
  // clone-directory setting; edits are pushed back to the setting on use.
  const defaultDest = codeFolder ?? cloneDirectory ?? ''
  const [url, setUrl] = useState('')
  const [dest, setDest] = useState(defaultDest)
  // Local folder name. Auto-follows the URL until the user edits it by hand.
  const [folderName, setFolderName] = useState('')
  const [folderTouched, setFolderTouched] = useState(false)
  const [cloning, setCloning] = useState(false)
  const [progress, setProgress] = useState('')
  const [openingGroupId, setOpeningGroupId] = useState<string | null>(null)

  useEffect(() => {
    setDest(defaultDest)
  }, [defaultDest])

  useEffect(() => {
    if (!cloning) return
    const unlisten = listen<GitProgressPayload>('git-progress', (event) => {
      if (event.payload.operation === 'clone') setProgress(event.payload.line)
    })
    return () => {
      unlisten.then((fn) => fn())
    }
  }, [cloning])

  const filteredScanned = useMemo(() => {
    const q = filter.toLowerCase()
    return (scanned.data ?? []).filter((r) => r.name.toLowerCase().includes(q))
  }, [scanned.data, filter])

  const filteredRecents = useMemo(() => {
    const q = filter.toLowerCase()
    const scannedPaths = new Set((scanned.data ?? []).map((r) => r.path.toLowerCase()))
    return recents.filter(
      (r) => r.name.toLowerCase().includes(q) && !scannedPaths.has(r.path.toLowerCase())
    )
  }, [recents, scanned.data, filter])

  const pickCodeFolder = async () => {
    const { open: openDialog } = await import('@tauri-apps/plugin-dialog')
    const dir = await openDialog({ directory: true, title: 'Select your code folder' })
    if (typeof dir === 'string') setCodeFolder(dir)
  }

  const browseForRepo = async () => {
    const { open: openDialog } = await import('@tauri-apps/plugin-dialog')
    const dir = await openDialog({ directory: true, title: 'Open repository' })
    if (typeof dir === 'string') openRepo.mutate(dir)
  }

  const suggestedName =
    url.trim().replace(/\/+$/, '').split('/').pop()?.replace(/\.git$/, '') ?? ''

  // Keep the folder name in sync with the URL until the user overrides it.
  useEffect(() => {
    if (!folderTouched) setFolderName(suggestedName)
  }, [suggestedName, folderTouched])

  const finalName = folderName.trim() || suggestedName

  const doClone = async () => {
    if (!url.trim() || !finalName || !dest.trim()) return
    const base = normalizePath(dest)
    setCloneDirectory(base)
    const destination = joinPath(base, finalName)
    setCloning(true)
    setProgress('Starting clone…')
    try {
      const cloned = unwrap(await commands.gitClone(url.trim(), destination))
      const repo = unwrap(await commands.openRepo(cloned))
      addRepo(repo)
      toast.success(`Cloned ${repo.name}`)
      closeModal()
      setUrl('')
      setFolderName('')
      setFolderTouched(false)
    } catch (e) {
      toast.error((e as Error).message)
    } finally {
      setCloning(false)
      setProgress('')
    }
  }

  const openSavedGroup = async (groupId: string) => {
    const group = savedTabGroups.find((candidate) => candidate.id === groupId)
    if (!group || openingGroupId) return
    setOpeningGroupId(groupId)
    const toastId = toast.loading(`Opening ${group.name}…`)
    const openedPaths: string[] = []
    const failures: string[] = []
    try {
      for (const path of group.repoPaths) {
        const alreadyOpen = useWorkspaceStore.getState().openRepos.find(
          (repo) => normalizePath(repo.path).toLowerCase() === normalizePath(path).toLowerCase(),
        )
        if (alreadyOpen) {
          openedPaths.push(alreadyOpen.path)
          continue
        }
        try {
          const repo = unwrap(await commands.openRepo(path))
          addRepo(repo)
          openedPaths.push(repo.path)
        } catch {
          failures.push(path)
        }
      }
      if (openedPaths.length > 0) {
        useWorkspaceStore.getState().createTabGroup(openedPaths, {
          id: group.id,
          name: group.name,
          color: group.color,
        })
        closeModal()
      }
      if (failures.length === 0) {
        toast.success(`Opened ${group.name} with ${openedPaths.length} repositories`)
      } else if (openedPaths.length > 0) {
        toast.warning(`Opened ${group.name}, but ${failures.length} ${failures.length === 1 ? 'repository was' : 'repositories were'} unavailable`)
      } else {
        toast.error(`None of the repositories in ${group.name} could be opened`)
      }
    } finally {
      toast.dismiss(toastId)
      setOpeningGroupId(null)
    }
  }

  return (
    <Dialog open={open} onOpenChange={(o) => !o && !cloning && closeModal()}>
      <DialogContent className="gap-0 p-0 sm:max-w-lg" aria-describedby={undefined}>
        <DialogHeader className="border-b border-border px-4 pb-3 pt-4">
          <DialogTitle className="text-sm">Open a repository</DialogTitle>
          <div className="mt-2 flex gap-1">
            {(
              [
                ['open', 'Open', <FolderGit2 key="i" size={13} />],
                ['clone', 'Clone', <Download key="i" size={13} />],
                ['groups', 'Groups', <Layers3 key="i" size={13} />],
              ] as [Tab, string, React.ReactNode][]
            ).map(([key, label, icon]) => (
              <button
                key={key}
                onClick={() => setTab(key)}
                className={cn(
                  'flex items-center gap-1.5 rounded-md px-3 py-1.5 text-xs font-medium',
                  tab === key
                    ? 'bg-soft text-primary'
                    : 'text-sub hover:bg-panel3 hover:text-foreground'
                )}
              >
                {icon}
                {label}
              </button>
            ))}
          </div>
        </DialogHeader>

        {tab === 'open' && (
          <div className="flex max-h-[420px] flex-col">
            <div className="flex gap-2 px-4 pt-3">
              <Input
                value={filter}
                onChange={(e) => setFilter(e.target.value)}
                placeholder="Filter repositories…"
                className="h-8 bg-background text-xs"
                autoFocus
              />
              <Button
                variant="secondary"
                size="sm"
                className="h-8 flex-none gap-1.5 text-xs"
                onClick={browseForRepo}
                disabled={openRepo.isPending}
              >
                <FolderSearch size={13} />
                Browse…
              </Button>
            </div>

            <div className="min-h-0 flex-1 overflow-y-auto px-1.5 pb-3">
              <div className="flex items-center">
                <SectionLabel>
                  CODE FOLDER{codeFolder ? ` · ${codeFolder}` : ''}
                </SectionLabel>
                <div className="flex-1" />
                {codeFolder && (
                  <button
                    onClick={() => scanned.refetch()}
                    title="Rescan"
                    className="mr-2 mt-2 rounded p-1 text-muted-foreground hover:bg-panel3 hover:text-foreground"
                  >
                    <RefreshCw size={11} className={cn(scanned.isFetching && 'animate-spin')} />
                  </button>
                )}
                <button
                  onClick={pickCodeFolder}
                  className="mr-2.5 mt-2 rounded px-1.5 py-0.5 text-[10px] text-primary hover:bg-soft"
                >
                  {codeFolder ? 'Change' : 'Select…'}
                </button>
              </div>
              {!codeFolder && (
                <div className="px-2.5 py-1 text-[11px] text-muted-foreground">
                  Select a folder like <span className="font-mono">C:\code</span> to quick-launch
                  every repository inside it.
                </div>
              )}
              {codeFolder && scanned.isLoading && (
                <div className="px-2.5 py-1 text-[11px] text-muted-foreground">Scanning…</div>
              )}
              {codeFolder && scanned.isError && (
                <div className="px-2.5 py-1 text-[11px] text-removed">
                  {(scanned.error as Error).message}
                </div>
              )}
              {filteredScanned.map((r) => (
                <RepoRow
                  key={r.path}
                  icon={<FolderGit2 size={15} strokeWidth={1.8} />}
                  name={r.name}
                  detail={r.path}
                  meta={openRepoIds.has(r.path.toLowerCase()) ? 'open' : (r.head_branch ?? undefined)}
                  onClick={() => openRepo.mutate(r.path)}
                  disabled={openRepo.isPending}
                />
              ))}

              {filteredRecents.length > 0 && (
                <>
                  <SectionLabel>RECENT</SectionLabel>
                  {filteredRecents.map((r) => (
                    <RepoRow
                      key={r.path}
                      icon={<Clock size={14} strokeWidth={1.8} />}
                      name={r.name}
                      detail={r.path}
                      onClick={() => openRepo.mutate(r.path)}
                      disabled={openRepo.isPending}
                    />
                  ))}
                </>
              )}
            </div>
          </div>
        )}

        {tab === 'clone' && (
          <div className="grid gap-3 px-4 py-4">
            <div className="grid gap-1.5">
              <label className="text-xs font-semibold text-sub">Repository URL</label>
              <Input
                value={url}
                onChange={(e) => setUrl(e.target.value)}
                placeholder="https://github.com/user/repo.git"
                className="h-8 bg-background font-mono text-xs"
                disabled={cloning}
                autoFocus
              />
            </div>
            <div className="grid gap-1.5">
              <label className="text-xs font-semibold text-sub">Destination folder</label>
              <div className="flex gap-2">
                <Input
                  value={dest}
                  onChange={(e) => setDest(e.target.value)}
                  onBlur={() => dest.trim() && setDest(normalizePath(dest))}
                  placeholder="Where new clones go"
                  className="h-8 bg-background font-mono text-xs"
                  disabled={cloning}
                />
                <Button
                  variant="secondary"
                  size="sm"
                  className="h-8 flex-none"
                  title="Browse for folder"
                  disabled={cloning}
                  onClick={async () => {
                    const { open: openDialog } = await import('@tauri-apps/plugin-dialog')
                    const dir = await openDialog({ directory: true, title: 'Clone into…' })
                    if (typeof dir === 'string') setDest(normalizePath(dir))
                  }}
                >
                  <Folder size={13} />
                </Button>
              </div>
            </div>
            <div className="grid gap-1.5">
              <label className="text-xs font-semibold text-sub">Folder name</label>
              <Input
                value={folderName}
                onChange={(e) => {
                  setFolderName(e.target.value)
                  setFolderTouched(true)
                }}
                placeholder={suggestedName || 'my-repo'}
                className="h-8 bg-background font-mono text-xs"
                disabled={cloning}
              />
              <p className="text-[11px] text-muted-foreground">
                The name of the folder created on your computer. Defaults to the repository name.
              </p>
            </div>
            {finalName && dest.trim() && (
              <div className="rounded-md border border-border bg-background px-3 py-2">
                <div className="text-[10px] font-semibold uppercase tracking-wide text-muted-foreground">
                  Clones into
                </div>
                <div className="mt-0.5 break-all font-mono text-xs text-foreground">
                  {joinPath(dest, finalName)}
                </div>
              </div>
            )}
            {cloning && (
              <div className="overflow-hidden text-ellipsis whitespace-nowrap font-mono text-[10.5px] text-muted-foreground">
                {progress}
              </div>
            )}
            <Button
              size="sm"
              disabled={!url.trim() || !finalName || !dest.trim() || cloning}
              onClick={doClone}
              className="mt-1"
            >
              {cloning ? 'Cloning…' : 'Clone repository'}
            </Button>
          </div>
        )}

        {tab === 'groups' && (
          <div className="max-h-[420px] min-h-48 overflow-y-auto px-1.5 pb-3">
            <SectionLabel>SAVED GROUPS</SectionLabel>
            {savedTabGroups.length === 0 ? (
              <div className="mx-2.5 mt-2 rounded-md border border-dashed border-border px-5 py-8 text-center">
                <Layers3 size={22} strokeWidth={1.5} className="mx-auto mb-2 text-muted-foreground" />
                <div className="text-xs font-semibold text-foreground">No saved groups yet</div>
                <p className="mt-1 text-[11px] leading-4 text-muted-foreground">
                  Right-click an open group and choose Save group.
                </p>
              </div>
            ) : savedTabGroups.map((group) => {
              const opening = openingGroupId === group.id
              return (
                <div
                  key={group.id}
                  className="group/saved flex items-center gap-2.5 rounded-md px-2.5 py-2 hover:bg-panel3"
                >
                  <span
                    className="grid size-8 flex-none place-items-center rounded-md border border-border bg-background"
                    style={{ color: group.color }}
                  >
                    <Layers3 size={15} strokeWidth={1.9} />
                  </span>
                  <span className="min-w-0 flex-1">
                    <span className="block text-xs font-semibold text-foreground">{group.name}</span>
                    <span className="block overflow-hidden text-ellipsis whitespace-nowrap font-mono text-[9.5px] text-muted-foreground">
                      {group.repoPaths.map((path) => path.split('\\').pop() ?? path).join(' · ')}
                    </span>
                  </span>
                  <span className="flex-none text-[10px] text-sub">
                    {group.repoPaths.length} {group.repoPaths.length === 1 ? 'repo' : 'repos'}
                  </span>
                  <TooltipButton
                    tooltip={`Delete ${group.name}`}
                    onClick={() => {
                      deleteSavedTabGroup(group.id)
                      toast.success(`${group.name} removed from saved groups`)
                    }}
                    disabled={openingGroupId != null}
                    className="grid size-7 flex-none place-items-center rounded-[5px] text-muted-foreground opacity-0 hover:bg-background hover:text-removed group-hover/saved:opacity-100"
                  >
                    <Trash2 size={12} />
                  </TooltipButton>
                  <Button
                    size="sm"
                    className="h-7 min-w-20 gap-1.5 text-[11px]"
                    disabled={openingGroupId != null}
                    onClick={() => void openSavedGroup(group.id)}
                  >
                    {opening ? <Loader2 size={12} className="animate-spin" /> : null}
                    {opening ? 'Opening…' : 'Open group'}
                  </Button>
                </div>
              )
            })}
          </div>
        )}
      </DialogContent>
    </Dialog>
  )
}
