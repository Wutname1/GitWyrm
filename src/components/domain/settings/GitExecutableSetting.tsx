import { useEffect, useRef, useState } from 'react'
import { Check, FileSearch, Loader2, X } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { commands } from '@/lib/bindings'
import { useWorkspaceStore } from '@/stores/workspaceStore'

type GitCheck = { state: 'checking' } | { state: 'ok'; version: string } | { state: 'error'; message: string }

/**
 * Git executable picker: a path input, a Browse button, and a live check that
 * runs `<path> --version` so the user sees whether their choice works. Empty
 * means "use git from PATH", which is still verified so a missing PATH git is
 * surfaced too.
 */
export function GitExecutableSetting() {
  const gitExecutable = useWorkspaceStore((s) => s.gitExecutable)
  const setGitExecutable = useWorkspaceStore((s) => s.setGitExecutable)
  const [draft, setDraft] = useState(gitExecutable)
  const [check, setCheck] = useState<GitCheck>({ state: 'checking' })

  // Keep the local draft in step if the value changes elsewhere (e.g. a reset).
  useEffect(() => {
    setDraft(gitExecutable)
  }, [gitExecutable])

  // Verify the committed value (not the in-progress draft) whenever it changes.
  const runId = useRef(0)
  useEffect(() => {
    const id = ++runId.current
    setCheck({ state: 'checking' })
    commands
      .verifyGitExecutable(gitExecutable)
      .then((res) => {
        if (id !== runId.current) return
        if (res.status === 'ok') setCheck({ state: 'ok', version: res.data })
        else setCheck({ state: 'error', message: res.error })
      })
      .catch((e: unknown) => {
        if (id !== runId.current) return
        setCheck({ state: 'error', message: e instanceof Error ? e.message : String(e) })
      })
  }, [gitExecutable])

  const commit = (raw: string) => {
    const next = raw.trim()
    setDraft(next)
    if (next !== gitExecutable) setGitExecutable(next)
  }

  const browse = async () => {
    const { open: openDialog } = await import('@tauri-apps/plugin-dialog')
    const picked = await openDialog({
      title: 'Select the git executable',
      multiple: false,
      directory: false,
      filters: [{ name: 'git', extensions: ['exe'] }],
    })
    if (typeof picked === 'string') commit(picked)
  }

  return (
    <div className="grid gap-1.5">
      <div className="flex gap-2">
        <Input
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onBlur={() => commit(draft)}
          onKeyDown={(e) => e.key === 'Enter' && commit(draft)}
          placeholder="git"
          className="h-8 bg-background font-mono text-xs"
          aria-label="Git executable path"
        />
        <Button
          variant="secondary"
          size="sm"
          className="h-8 flex-none"
          tooltip="Browse for git.exe"
          onClick={browse}
        >
          <FileSearch size={13} />
          Browse
        </Button>
      </div>
      <CheckLine check={check} usingPath={gitExecutable.trim().length > 0} />
    </div>
  )
}

function CheckLine({ check, usingPath }: { check: GitCheck; usingPath: boolean }) {
  if (check.state === 'checking') {
    return (
      <div className="flex items-center gap-1.5 text-2xs text-muted-foreground">
        <Loader2 size={12} className="animate-spin" />
        Checking git...
      </div>
    )
  }
  if (check.state === 'ok') {
    return (
      <div className="flex items-center gap-1.5 text-2xs text-emerald-500">
        <Check size={12} className="flex-none" />
        <span className="truncate">
          {check.version}
          {!usingPath && <span className="text-muted-foreground"> (from PATH)</span>}
        </span>
      </div>
    )
  }
  return (
    <div className="flex items-start gap-1.5 text-2xs text-red-400">
      <X size={12} className="mt-0.5 flex-none" />
      <span>{check.message}</span>
    </div>
  )
}
