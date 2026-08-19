import { useEffect, useMemo, useState } from 'react'
import { Package } from 'lucide-react'
import { Input } from '@/components/ui/input'
import { FormDialog } from '@/components/ui/form-dialog'
import { useSubmodules } from '@/hooks/useGitQueries'
import { useGitMutations } from '@/hooks/useGitMutations'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo } from '@/stores/workspaceStore'

/**
 * The folder name a clone URL would land in: the last path segment with any
 * `.git` suffix and trailing slashes removed. Used to prefill the path so the
 * common case needs one field, not two.
 */
function folderFromUrl(url: string): string {
  const trimmed = url.trim().replace(/\/+$/, '')
  if (trimmed === '') return ''
  const last = trimmed.split(/[/:]/).pop() ?? ''
  return last.replace(/\.git$/i, '')
}

export function AddSubmoduleModal() {
  const open = useUiStore((s) => s.activeModal === 'addSubmodule')
  const closeModal = useUiStore((s) => s.closeModal)

  const repo = useActiveRepo()
  const submodules = useSubmodules(repo?.id ?? null)
  const m = useGitMutations(repo?.id ?? null)

  const [url, setUrl] = useState('')
  const [path, setPath] = useState('')
  const [branch, setBranch] = useState('')
  /** True once the user edits the path themselves, which stops the URL prefill. */
  const [pathTouched, setPathTouched] = useState(false)

  useEffect(() => {
    if (open) {
      setUrl('')
      setPath('')
      setBranch('')
      setPathTouched(false)
    }
  }, [open])

  // Mirror the URL into the path until the user takes it over.
  useEffect(() => {
    if (!pathTouched) setPath(folderFromUrl(url))
  }, [url, pathTouched])

  const taken = useMemo(
    () => new Set((submodules.data ?? []).map((s) => s.path)),
    [submodules.data]
  )

  const trimmedUrl = url.trim()
  const trimmedPath = path.trim().replace(/\\/g, '/').replace(/^\/+|\/+$/g, '')

  const pathError =
    trimmedPath === ''
      ? null
      : taken.has(trimmedPath)
        ? 'This project already has a submodule in that folder.'
        : trimmedPath.startsWith('-')
          ? "Folder can't start with a dash."
          : trimmedPath.startsWith('../') || trimmedPath === '..'
            ? 'Folder has to be inside this project.'
            : null

  const canSubmit =
    trimmedUrl !== '' && trimmedPath !== '' && !pathError && !m.addSubmodule.isPending

  const submit = () => {
    if (!canSubmit) return
    m.addSubmodule.mutate(
      { url: trimmedUrl, path: trimmedPath, branch: branch.trim() || null },
      { onSuccess: () => closeModal() }
    )
  }

  return (
    <FormDialog
      open={open}
      onOpenChange={(o) => !o && closeModal()}
      icon={<Package size={15} strokeWidth={1.9} />}
      title="Add a submodule"
      submitLabel="Add submodule"
      pendingLabel="Adding…"
      canSubmit={canSubmit}
      pending={m.addSubmodule.isPending}
      onSubmit={submit}
    >
      <div className="grid gap-1.5">
        <label className="text-2xs font-semibold text-sub">Address</label>
        <Input
          value={url}
          onChange={(e) => setUrl(e.target.value)}
          placeholder="https://github.com/someone/their-project.git"
          className="h-auto bg-background py-1.5 font-mono text-xs"
          autoFocus
        />
        <p className="text-2xs leading-tight text-muted-foreground">
          The web address of the project you want to include.
        </p>
      </div>

      <div className="grid gap-1.5">
        <label className="text-2xs font-semibold text-sub">Folder</label>
        <Input
          value={path}
          onChange={(e) => {
            setPathTouched(true)
            setPath(e.target.value)
          }}
          placeholder="packages/their-project"
          className="h-auto bg-background py-1.5 font-mono text-xs"
        />
        <p className="min-h-[15px] text-2xs leading-tight text-removed">{pathError ?? ''}</p>
        <p className="text-2xs leading-tight text-muted-foreground">
          Where it goes inside this project.
        </p>
      </div>

      <div className="grid gap-1.5">
        <label className="text-2xs font-semibold text-sub">
          Branch <span className="font-normal text-muted-foreground">(optional)</span>
        </label>
        <Input
          value={branch}
          onChange={(e) => setBranch(e.target.value)}
          placeholder="main"
          className="h-auto bg-background py-1.5 font-mono text-xs"
        />
        <p className="text-2xs leading-tight text-muted-foreground">
          The branch to follow when you update it later. Leave blank to use its
          default branch.
        </p>
      </div>

      <p className="text-2xs text-muted-foreground">
        This downloads the project and adds it to your changes. Commit to save it.
      </p>
    </FormDialog>
  )
}
