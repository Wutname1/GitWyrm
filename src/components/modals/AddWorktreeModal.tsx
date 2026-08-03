import { useEffect, useMemo, useState } from 'react'
import { open as openFolderDialog } from '@tauri-apps/plugin-dialog'
import { FolderGit2, FolderOpen } from 'lucide-react'
import { Input } from '@/components/ui/input'
import { FormDialog } from '@/components/ui/form-dialog'
import { commands, type IgnoredFile, type Worktree } from '@/lib/bindings'
import { unwrap } from '@/lib/queryKeys'
import { samePath } from '@/lib/paths'
import { branchHeldMessage } from '@/lib/worktreeCopy'
import { useBranches, useWorktrees } from '@/hooks/useGitQueries'
import { useGitMutations } from '@/hooks/useGitMutations'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo, useWorkspaceStore } from '@/stores/workspaceStore'
import { useOpenRepo } from '@/hooks/useRepoActions'

/** A readable size for the copy list -- "2 KB" beats "2048". */
function readableSize(bytes: number): string {
  if (bytes < 1024) return `${Math.round(bytes)} B`
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
}

/**
 * Make a second folder of this project, checked out to another branch.
 *
 * The dialog states the full path before anything is created, warns when the
 * chosen folder sits inside the project, refuses a folder that already has
 * files, and offers to carry across the ignored local files a fresh checkout
 * would otherwise be missing -- the single most common reason a new worktree
 * looks broken for a reason git never mentions.
 */
export function AddWorktreeModal() {
  const open = useUiStore((s) => s.activeModal === 'addWorktree')
  const closeModal = useUiStore((s) => s.closeModal)

  const repo = useActiveRepo()
  const m = useGitMutations(repo?.id ?? null)
  const branches = useBranches(repo?.id ?? null)
  const { data: worktrees } = useWorktrees(repo?.id ?? null)
  const openRepo = useOpenRepo()
  const setActiveRepo = useWorkspaceStore((s) => s.setActiveRepo)
  const tabs = useWorkspaceStore((s) => s.openRepos)

  const [branch, setBranch] = useState('')
  const [path, setPath] = useState('')
  /** True once the user edits the folder themselves, stopping the suggestion. */
  const [pathTouched, setPathTouched] = useState(false)
  const [pathProblem, setPathProblem] = useState<string | null>(null)
  const [copyable, setCopyable] = useState<IgnoredFile[]>([])
  const [chosen, setChosen] = useState<Set<string>>(new Set())

  const local = branches.data?.local ?? []
  const existingNames = useMemo(() => new Set(local.map((b) => b.name)), [local])
  const trimmedBranch = branch.trim()
  const isNewBranch = trimmedBranch !== '' && !existingNames.has(trimmedBranch)

  // Which commit a new branch starts from. Saying so is the difference between
  // "make a branch" and knowing what will be in it.
  const startPoint = useMemo(
    () => local.find((b) => b.is_head)?.name ?? 'main',
    [local]
  )

  /** The worktree already holding this branch, if any. */
  const holder: Worktree | null = useMemo(() => {
    if (isNewBranch || trimmedBranch === '') return null
    return (worktrees ?? []).find((w) => w.state === 'ok' && w.branch === trimmedBranch) ?? null
  }, [worktrees, trimmedBranch, isNewBranch])

  useEffect(() => {
    if (!open) return
    setBranch('')
    setPath('')
    setPathTouched(false)
    setPathProblem(null)
    setChosen(new Set())
    // The ignored-file survey is per-project, not per-branch, so it runs once
    // when the dialog opens.
    if (repo) {
      void commands
        .worktreeCopyableFiles(repo.id)
        .then(unwrap)
        .then((files) => {
          setCopyable(files)
          // Pre-tick everything: the user opted into the offer by having these
          // files, and unticking is easier than hunting for the one they need.
          setChosen(new Set(files.map((f) => f.path)))
        })
        .catch(() => setCopyable([]))
    }
  }, [open, repo])

  // Suggest a sibling folder until the user takes the field over. Computed in
  // the backend so every caller agrees on where worktrees go.
  useEffect(() => {
    if (!open || !repo || pathTouched || trimmedBranch === '') return
    let cancelled = false
    void commands
      .suggestWorktreePath(repo.id, trimmedBranch)
      .then(unwrap)
      .then((suggested) => {
        if (!cancelled) setPath(suggested)
      })
      .catch(() => {})
    return () => {
      cancelled = true
    }
  }, [open, repo, trimmedBranch, pathTouched])

  // Validate the folder as it is typed, so a bad choice is a sentence here
  // rather than a half-created folder to clean up afterwards.
  useEffect(() => {
    if (!open || !repo || path.trim() === '') {
      setPathProblem(null)
      return
    }
    let cancelled = false
    const timer = setTimeout(() => {
      void commands
        .checkWorktreePath(repo.id, path.trim())
        .then(unwrap)
        .then((problem) => {
          if (!cancelled) setPathProblem(problem)
        })
        .catch(() => {})
    }, 200)
    return () => {
      cancelled = true
      clearTimeout(timer)
    }
  }, [open, repo, path])

  async function pickFolder() {
    const picked = await openFolderDialog({
      directory: true,
      multiple: false,
      title: 'Where should the new folder go?',
    })
    if (typeof picked === 'string') {
      setPathTouched(true)
      setPath(picked)
    }
  }

  function openHolder() {
    if (!holder) return
    const existing = tabs.find((r) => samePath(r.path, holder.path))
    if (existing) {
      setActiveRepo(existing.id)
      closeModal()
      return
    }
    void openRepo.mutateAsync(holder.path).then(() => closeModal())
  }

  const canSubmit =
    trimmedBranch !== '' &&
    path.trim() !== '' &&
    pathProblem == null &&
    holder == null &&
    !m.addWorktree.isPending

  const submit = () => {
    if (!canSubmit) return
    m.addWorktree.mutate(
      {
        path: path.trim(),
        branch: trimmedBranch,
        newBranch: isNewBranch,
        startPoint: isNewBranch ? startPoint : null,
        copyFiles: [...chosen],
      },
      { onSuccess: () => closeModal() }
    )
  }

  return (
    <FormDialog
      open={open}
      onOpenChange={(o) => !o && closeModal()}
      icon={<FolderGit2 size={15} strokeWidth={1.9} />}
      title="Work on another branch in its own folder"
      submitLabel="Make the folder"
      pendingLabel="Making…"
      canSubmit={canSubmit}
      pending={m.addWorktree.isPending}
      widthClassName="sm:max-w-lg"
      onSubmit={submit}
    >
      <p className="text-2xs leading-relaxed text-muted-foreground">
        This makes a second copy of your project's files on your computer, on a different branch.
        Both stay in sync with the same history, so you can work in one while the other sits
        untouched.
      </p>

      <div className="grid gap-1.5">
        <label className="text-2xs font-semibold text-sub">Branch</label>
        <Input
          value={branch}
          onChange={(e) => setBranch(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') submit()
          }}
          list="worktree-branches"
          placeholder="feature/login"
          className="h-auto bg-background py-1.5 font-mono text-xs"
          autoFocus
        />
        <datalist id="worktree-branches">
          {local.map((b) => (
            <option key={b.name} value={b.name} />
          ))}
        </datalist>
        {holder ? (
          <p className="text-2xs leading-tight text-[var(--gw-amber)]">
            {branchHeldMessage(trimmedBranch, holder.folder_name)}{' '}
            <button
              type="button"
              onClick={openHolder}
              className="underline underline-offset-2 hover:text-foreground"
            >
              Open that folder
            </button>
          </p>
        ) : isNewBranch ? (
          <p className="text-2xs leading-tight text-muted-foreground">
            No branch called <span className="font-mono text-foreground">{trimmedBranch}</span> yet
            — it will be made, starting from{' '}
            <span className="font-mono text-foreground">{startPoint}</span>.
          </p>
        ) : (
          <p className="text-2xs leading-tight text-muted-foreground">
            Pick a branch you already have, or type a new name to make one.
          </p>
        )}
      </div>

      <div className="grid gap-1.5">
        <label className="text-2xs font-semibold text-sub">Folder</label>
        <div className="flex gap-1.5">
          <Input
            value={path}
            onChange={(e) => {
              setPathTouched(true)
              setPath(e.target.value)
            }}
            onKeyDown={(e) => {
              if (e.key === 'Enter') submit()
            }}
            placeholder="C:/code/project-feature"
            className="h-auto flex-1 bg-background py-1.5 font-mono text-xs"
          />
          <button
            type="button"
            onClick={() => void pickFolder()}
            className="flex flex-none items-center gap-1 rounded border border-border bg-panel2 px-2 text-2xs text-sub hover:bg-panel3 hover:text-foreground"
          >
            <FolderOpen size={11} />
            Browse
          </button>
        </div>
        {pathProblem ? (
          <p className="text-2xs leading-tight text-removed">{pathProblem}</p>
        ) : path.trim() !== '' ? (
          // Name the exact path before the user commits to it -- a folder
          // appearing somewhere they did not expect is the surprise this
          // prevents.
          <p className="text-2xs leading-tight text-muted-foreground">
            Files will be created in{' '}
            <span className="font-mono text-foreground">{path.trim()}</span>.
          </p>
        ) : (
          <p className="text-2xs leading-tight text-muted-foreground">
            Suggested next to your project, so it never gets committed into it by accident.
          </p>
        )}
      </div>

      {/* A fresh copy has only the files git tracks, so local setup files are
          missing and the new folder looks broken for a reason git never
          mentions. Absent entirely when there is nothing to carry. */}
      {copyable.length > 0 && (
        <div className="grid gap-1.5">
          <label className="text-2xs font-semibold text-sub">Bring your setup files across</label>
          <div className="max-h-28 overflow-y-auto rounded border border-border bg-background p-1.5">
            {copyable.map((f) => (
              <label
                key={f.path}
                className="flex cursor-pointer items-center gap-2 py-0.5 text-2xs text-sub hover:text-foreground"
              >
                <input
                  type="checkbox"
                  checked={chosen.has(f.path)}
                  onChange={(e) => {
                    const next = new Set(chosen)
                    if (e.target.checked) next.add(f.path)
                    else next.delete(f.path)
                    setChosen(next)
                  }}
                  className="size-3 flex-none accent-[var(--gw-accent)]"
                />
                <span className="truncate font-mono">{f.path}</span>
                <span className="ml-auto flex-none text-muted-foreground">
                  {readableSize(f.size_bytes)}
                </span>
              </label>
            ))}
          </div>
          <p className="text-2xs leading-tight text-muted-foreground">
            These are files git doesn't track — settings and keys the project needs to run. Installed
            packages aren't copied; you'll need to install them in the new folder before it runs.
          </p>
        </div>
      )}
    </FormDialog>
  )
}
