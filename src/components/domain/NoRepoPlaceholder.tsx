import { FolderOpen, Loader2, Plus } from 'lucide-react'
import logoUrl from '@/assets/logo.png'
import { useOpenRepo } from '@/hooks/useRepoActions'
import { useUiStore } from '@/stores/uiStore'
import { useWorkspaceStore } from '@/stores/workspaceStore'

/**
 * Fills the center panel when no repository is open. The whole point is that
 * the way in is impossible to miss, so the button is large and centered rather
 * than a line of grey text pointing at the small one up in the tab bar.
 */
export function NoRepoPlaceholder() {
  const openModal = useUiStore((s) => s.openModal)
  const recents = useWorkspaceStore((s) => s.recents)
  const openRepo = useOpenRepo()

  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-5 p-8">
      <img src={logoUrl} alt="" draggable={false} className="size-12 opacity-70" />
      <div className="text-center">
        <h2 className="text-base font-semibold text-foreground">No repository open</h2>
        <p className="mt-1 max-w-sm text-xs text-muted-foreground">
          Open a folder that is already tracked by git, or download a copy of one from
          the internet.
        </p>
      </div>
      <button
        type="button"
        onClick={() => openModal('clone')}
        disabled={openRepo.isPending}
        className="flex h-10 items-center gap-2 rounded-[7px] bg-primary px-5 text-sm font-medium text-primary-foreground transition-opacity hover:opacity-90 disabled:opacity-60"
      >
        {openRepo.isPending ? (
          <Loader2 size={16} className="animate-spin" />
        ) : (
          <Plus size={16} />
        )}
        Open a repository
      </button>

      {recents.length > 0 && (
        <div className="w-full max-w-sm">
          <div className="mb-1.5 text-center text-2xs font-semibold tracking-[.09em] text-muted-foreground">
            RECENT
          </div>
          <div className="flex flex-col gap-0.5">
            {recents.slice(0, 5).map((repo) => (
              <button
                key={repo.path}
                type="button"
                onClick={() => openRepo.mutate(repo.path)}
                disabled={openRepo.isPending}
                className="flex w-full items-center gap-2 rounded-[5px] px-2.5 py-1.5 text-left hover:bg-panel2 disabled:opacity-60"
              >
                <FolderOpen size={13} strokeWidth={2} className="flex-none text-sub" />
                <span className="min-w-0 flex-1">
                  <span className="block truncate text-xs text-foreground">{repo.name}</span>
                  <span className="block truncate font-mono text-2xs text-muted-foreground">
                    {repo.path}
                  </span>
                </span>
              </button>
            ))}
          </div>
        </div>
      )}
    </div>
  )
}
