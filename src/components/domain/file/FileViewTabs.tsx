import { cn } from '@/lib/utils'
import { Button } from '@/components/ui/button'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo } from '@/stores/workspaceStore'
import { useNeverCommitted } from '@/hooks/useGitQueries'
import type { DiffSource } from '@/lib/bindings'

export type FileViewMode = 'diff' | 'history' | 'blame'

/**
 * Switches between the three ways of looking at one file. Shared by the diff
 * header and the history / blame header so the control sits in the same place
 * and behaves the same way in all three.
 *
 * Diff follows the commit you are looking at: from a past commit it shows that
 * commit's own change to the file, and with no commit in context -- the file
 * came from pending changes -- it falls back to the working tree.
 */
export function FileViewTabs({ path, mode }: { path: string; mode: FileViewMode }) {
  const openDiff = useUiStore((s) => s.openDiff)
  const openFileHistory = useUiStore((s) => s.openFileHistory)
  const openBlame = useUiStore((s) => s.openBlame)
  const sha = useUiStore((s) => s.fileTarget?.sha ?? null)
  const diffRequest = useUiStore((s) => s.diffRequest)
  const repo = useActiveRepo()
  // History and blame both read from commits, so they are useless for a file
  // that has never been committed.
  const neverCommitted = useNeverCommitted(repo?.id ?? null, path, sha)

  // Prefer the commit the file view is pinned to; otherwise stay on whatever
  // source the diff is already showing, so switching away and back does not
  // silently retarget a staged diff at the working tree.
  const source: DiffSource =
    sha != null
      ? { kind: 'commit', sha }
      : diffRequest?.path === path
        ? diffRequest.source
        : { kind: 'unstaged' }

  const tabs = (
    [
      ['diff', 'Diff', () => openDiff({ path, source })],
      ['history', 'History', () => openFileHistory(path)],
      ['blame', 'Blame', () => openBlame(path)],
    ] as const
  ).filter(([key]) => key === 'diff' || !neverCommitted)

  // A lone Diff tab is a label, not a choice.
  if (tabs.length < 2) return null

  return (
    <div className="flex flex-none items-center rounded border border-border bg-panel2 p-px">
      {tabs.map(([key, label, go]) => (
        <Button
          key={key}
          size="sm"
          variant="ghost"
          onClick={go}
          aria-pressed={mode === key}
          className={cn(
            'h-auto rounded-[3px] px-2 py-0.5 text-2xs font-semibold',
            mode === key
              ? 'bg-soft text-accent-text hover:bg-soft'
              : 'text-sub hover:bg-panel3 hover:text-foreground'
          )}
        >
          {label}
        </Button>
      ))}
    </div>
  )
}
