import { useEffect, useState } from 'react'
import {
  Code2,
  EyeOff,
  FolderGit2,
  FolderOpen,
  GitBranch,
  ImageIcon,
  Loader2,
  RotateCcw,
  Save,
  SquareTerminal,
} from 'lucide-react'
import { toast } from 'sonner'
import { RepoIconDialog } from '@/components/domain/RepoIconDialog'
import { SettingRow } from '@/components/domain/settings/SettingRow'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { commands, type RepoIcon, type Result } from '@/lib/bindings'
import { normalizePath } from '@/lib/paths'
import { unwrap } from '@/lib/queryKeys'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo, useWorkspaceStore } from '@/stores/workspaceStore'

type OpeningTarget = 'folder' | 'editor' | 'terminal'

function messageFrom(error: unknown): string {
  return error instanceof Error ? error.message : String(error)
}

export function RepositorySettings() {
  const repo = useActiveRepo()
  const tabAliases = useWorkspaceStore((state) => state.tabAliases)
  const showRepoIcons = useWorkspaceStore((state) => state.showRepoIcons)
  const setTabAlias = useWorkspaceStore((state) => state.setTabAlias)
  const iconRevision = useWorkspaceStore((state) => repo
    ? state.repoIconRevisions[normalizePath(repo.path).toLowerCase()] ?? 0
    : 0)
  const [aliasDraft, setAliasDraft] = useState('')
  const [icon, setIcon] = useState<RepoIcon | null>(null)
  const [iconLoading, setIconLoading] = useState(false)
  const [iconDialogOpen, setIconDialogOpen] = useState(false)
  const [opening, setOpening] = useState<OpeningTarget | null>(null)

  const currentAlias = repo ? tabAliases[repo.path] ?? '' : ''

  useEffect(() => {
    setAliasDraft(currentAlias)
  }, [currentAlias, repo?.path])

  useEffect(() => {
    if (!repo) {
      setIcon(null)
      return
    }
    let active = true
    setIconLoading(true)
    commands.getRepoIcon(repo.path)
      .then(unwrap)
      .then((selected) => {
        if (active) setIcon(selected)
      })
      .catch(() => {
        if (active) setIcon(null)
      })
      .finally(() => {
        if (active) setIconLoading(false)
      })
    return () => { active = false }
  }, [iconRevision, repo])

  if (!repo) {
    return (
      <div className="mt-4 grid place-items-center rounded-xl border border-dashed border-border bg-panel/40 px-6 py-12 text-center">
        <div className="grid size-12 place-items-center rounded-xl border border-border bg-panel2 text-muted-foreground">
          <FolderGit2 size={22} strokeWidth={1.6} />
        </div>
        <h3 className="mt-3 text-sm font-semibold text-foreground">No repository selected</h3>
        <p className="mt-1 max-w-sm text-2xs leading-relaxed text-muted-foreground">
          Select a repository tab first. Its name, icon, and folder tools will appear here.
        </p>
      </div>
    )
  }

  const displayName = currentAlias || repo.name
  const aliasChanged = aliasDraft.trim() !== currentAlias

  const saveAlias = () => {
    const nextAlias = aliasDraft.trim()
    setTabAlias(repo.path, nextAlias)
    setAliasDraft(nextAlias)
    toast.success(nextAlias ? `Tab name changed to ${nextAlias}` : `Tab name reset to ${repo.name}`)
  }

  const runShortcut = async (
    target: OpeningTarget,
    action: () => Promise<Result<null, string>>,
    successMessage: string,
  ) => {
    setOpening(target)
    try {
      unwrap(await action())
      toast.success(successMessage)
    } catch (error: unknown) {
      toast.error(`Could not open it: ${messageFrom(error)}`)
    } finally {
      setOpening(null)
    }
  }

  return (
    <div>
      <div className="mt-4 flex items-center gap-4 rounded-xl border border-border bg-panel px-4 py-3.5 shadow-sm">
        <button
          type="button"
          onClick={() => setIconDialogOpen(true)}
          className="group relative grid size-14 flex-none place-items-center overflow-hidden rounded-xl border border-border bg-background outline-none transition-colors hover:border-primary focus-visible:border-primary focus-visible:ring-2 focus-visible:ring-ring/40"
          aria-label={`Change icon for ${displayName}`}
        >
          {iconLoading
            ? <Loader2 size={20} className="animate-spin text-muted-foreground" />
            : icon
              ? <img src={icon.data_url} alt="" className="size-full object-cover" />
              : <FolderGit2 size={25} className="text-accent-text" strokeWidth={1.6} />}
          <span className="absolute inset-x-0 bottom-0 grid h-5 translate-y-full place-items-center bg-background/90 text-[9px] font-semibold text-foreground transition-transform group-hover:translate-y-0 group-focus-visible:translate-y-0">
            CHANGE
          </span>
        </button>
        <div className="min-w-0 flex-1">
          <div className="truncate text-base font-bold text-foreground">{displayName}</div>
          <div className="mt-0.5 truncate font-mono text-2xs text-muted-foreground">{repo.path}</div>
          {repo.head_branch && (
            <div className="mt-2 flex items-center gap-1.5 text-2xs text-sub">
              <GitBranch size={11} />
              <span className="truncate">{repo.head_branch}</span>
            </div>
          )}
        </div>
      </div>

      <SettingRow
        label="Tab name"
        hint="This changes only the name shown in GitWyrm. Your folder keeps its original name."
      >
        <form
          className="grid gap-2"
          onSubmit={(event) => {
            event.preventDefault()
            if (aliasChanged) saveAlias()
          }}
        >
          <div className="flex gap-2">
            <Input
              value={aliasDraft}
              onChange={(event) => setAliasDraft(event.target.value)}
              placeholder={repo.name}
              className="h-8 bg-background text-xs"
              aria-label="Repository tab name"
            />
            <Button type="submit" size="sm" disabled={!aliasChanged} className="h-8 flex-none">
              <Save size={13} />
              Save
            </Button>
          </div>
          {currentAlias && (
            <Button
              type="button"
              variant="ghost"
              size="xs"
              className="justify-self-start"
              onClick={() => {
                setAliasDraft('')
                setTabAlias(repo.path, '')
                toast.success(`Tab name reset to ${repo.name}`)
              }}
            >
              <RotateCcw size={12} />
              Use folder name
            </Button>
          )}
        </form>
      </SettingRow>

      <SettingRow
        label="Tab icon"
        hint="GitWyrm can find a favicon or logo in this repository, or you can choose your own image."
      >
        <div className="grid gap-2">
          <div className="flex items-center gap-3">
            <div className="grid size-9 flex-none place-items-center overflow-hidden rounded-lg border border-border bg-background">
              {icon
                ? <img src={icon.data_url} alt="" className="size-full object-cover" />
                : <ImageIcon size={16} className="text-muted-foreground" />}
            </div>
            <div className="min-w-0 flex-1">
              <div className="text-xs font-medium text-foreground">
                {icon?.custom ? 'Custom image' : icon ? 'Found in repository' : 'Default marker'}
              </div>
              {icon && <div className="truncate text-2xs text-muted-foreground">{icon.label}</div>}
            </div>
            <Button variant="secondary" size="sm" onClick={() => setIconDialogOpen(true)}>
              <ImageIcon size={13} />
              Change icon
            </Button>
          </div>
          {!showRepoIcons && (
            <div className="flex items-center gap-2 rounded-md border border-border bg-panel2 px-2.5 py-2 text-2xs text-muted-foreground">
              <EyeOff size={13} className="flex-none" />
              <span className="flex-1">Repository icons are hidden in every tab.</span>
              <Button
                variant="link"
                size="xs"
                onClick={() => useUiStore.getState().showSettings('appearance')}
              >
                Open Appearance
              </Button>
            </div>
          )}
        </div>
      </SettingRow>

      <SettingRow label="Repository folder" hint="Quick ways to open this repository outside GitWyrm.">
        <div className="grid gap-2">
          <Input readOnly value={repo.path} className="h-8 bg-background font-mono text-xs" />
          <div className="flex flex-wrap gap-2">
            <Button
              variant="secondary"
              size="sm"
              disabled={opening != null}
              onClick={() => void runShortcut(
                'folder',
                () => commands.revealInFileManager(repo.id),
                `Showed ${displayName} in File Explorer`,
              )}
            >
              {opening === 'folder' ? <Loader2 className="animate-spin" /> : <FolderOpen />}
              Show folder
            </Button>
            <Button
              variant="secondary"
              size="sm"
              disabled={opening != null}
              onClick={() => void runShortcut(
                'editor',
                () => commands.openInEditor(repo.id),
                `Opened ${displayName} in your editor`,
              )}
            >
              {opening === 'editor' ? <Loader2 className="animate-spin" /> : <Code2 />}
              Open in editor
            </Button>
            <Button
              variant="secondary"
              size="sm"
              disabled={opening != null}
              onClick={() => void runShortcut(
                'terminal',
                () => commands.openInTerminal(repo.id),
                `Opened a terminal for ${displayName}`,
              )}
            >
              {opening === 'terminal' ? <Loader2 className="animate-spin" /> : <SquareTerminal />}
              Open terminal
            </Button>
          </div>
        </div>
      </SettingRow>

      <RepoIconDialog
        repo={repo}
        open={iconDialogOpen}
        onOpenChange={setIconDialogOpen}
      />
    </div>
  )
}
