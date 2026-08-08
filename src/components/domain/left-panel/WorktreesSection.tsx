import { useEffect, useState } from 'react'
import { open as openFolderDialog } from '@tauri-apps/plugin-dialog'
import {
  AlertTriangle,
  ChevronRight,
  FolderGit2,
  FolderOpen,
  Plus,
  Sparkles,
  Trash2,
  Wrench,
} from 'lucide-react'
import { toast } from 'sonner'
import { cn } from '@/lib/utils'
import { commands, type DirtyCount, type Worktree } from '@/lib/bindings'
import { unwrap } from '@/lib/queryKeys'
import { samePath } from '@/lib/paths'
import {
  brokenAction,
  brokenExplanation,
  dirtySummary,
  hasUnrecoverableWork,
  linkedWorktrees,
  looksLikeMovedRepository,
  removeConfirmCopy,
  sectionVisibility,
  visibleWorktrees,
} from '@/lib/worktreeCopy'
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuLabel,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from '@/components/ui/context-menu'
import { PendingMenuItem } from '@/components/ui/pending-menu-item'
import { ConfirmDialog } from '@/components/modals/ConfirmDialog'
import { TooltipButton, TooltipHint } from '@/components/ui/tooltip'
import { SidebarTip } from '@/components/domain/SidebarTip'
import { useGitMutations } from '@/hooks/useGitMutations'
import { useWorktrees } from '@/hooks/useGitQueries'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo, useWorkspaceStore } from '@/stores/workspaceStore'
import { useOpenRepo } from '@/hooks/useRepoActions'

/** The row's right-edge label: what state it is in, and the colour for it. */
function stateLabel(w: Worktree): { text: string; tone: string } | null {
  if (w.state === 'missing') return { text: 'folder gone', tone: 'text-[var(--gw-amber)]' }
  if (w.state === 'moved') return { text: 'link broken', tone: 'text-[var(--gw-amber)]' }
  if (w.is_current) return { text: 'open', tone: 'text-[var(--gw-accent-text)]' }
  return null
}

function WorktreeRow({ worktree, canAdd }: { worktree: Worktree; canAdd: boolean }) {
  const repo = useActiveRepo()
  const m = useGitMutations(repo?.id ?? null)
  const openRepo = useOpenRepo()
  const tabs = useWorkspaceStore((s) => s.openRepos)
  const setActiveRepo = useWorkspaceStore((s) => s.setActiveRepo)

  const [confirmRemove, setConfirmRemove] = useState(false)
  const [dirt, setDirt] = useState<DirtyCount>({ modified: 0, untracked: 0 })
  const [discard, setDiscard] = useState(false)
  const [opening, setOpening] = useState(false)
  const [cleanupBranch, setCleanupBranch] = useState<string | null>(null)

  const label = stateLabel(worktree)
  const broken = worktree.state !== 'ok'
  const action = brokenAction(worktree.state)
  const busy = m.removeWorktree.isPending || m.pruneWorktrees.isPending || m.repairWorktree.isPending

  /**
   * Open this checkout as its own tab. A linked worktree is a real working
   * folder, so the ordinary open-by-path route carries it -- no special casing
   * needed anywhere downstream.
   */
  async function openAsTab() {
    if (opening || broken) return
    // Already open: switch to that tab rather than opening a second one.
    const existing = tabs.find((r) => samePath(r.path, worktree.path))
    if (existing) {
      setActiveRepo(existing.id)
      return
    }
    setOpening(true)
    try {
      await openRepo.mutateAsync(worktree.path)
    } catch {
      // useOpenRepo reports its own failure.
    } finally {
      setOpening(false)
    }
  }

  /**
   * Ask what is unsaved before showing the confirm, so the dialog can state the
   * counts rather than warning vaguely about "changes".
   */
  async function startRemove() {
    if (!repo) return
    try {
      const counts = await commands.worktreeDirtyCount(worktree.path).then(unwrap)
      setDirt(counts)
      setDiscard(false)
    } catch {
      // An unreadable folder still gets a confirm; it just cannot promise what
      // is in there.
      setDirt({ modified: 0, untracked: 0 })
    }
    setConfirmRemove(true)
  }

  function runRemove() {
    const clean = dirt.modified === 0 && dirt.untracked === 0
    m.removeWorktree.mutate(
      {
        path: worktree.path,
        folderName: worktree.folder_name,
        // A clean worktree needs no decision. A dirty one only ever reaches
        // here after this confirm, which is where "--force" comes from.
        choice: clean ? 'refuse' : discard ? 'discard' : 'keep',
      },
      {
        onSuccess: ({ outcome }) => {
          if (outcome.kind === 'removed' && outcome.branch && outcome.branch_merged) {
            // Cleaning up in two places is the step people forget, so offer the
            // second one -- but only when the branch has nothing of its own.
            setCleanupBranch(outcome.branch)
          }
          if (outcome.kind === 'refusedLocked') {
            toast.error('A program is still using that folder', {
              description:
                'Close any editor or terminal open in it, then try Remove again. GitWyrm has already let go of it.',
              action: { label: 'Try again', onClick: () => runRemove() },
            })
          }
          if (outcome.kind === 'partiallyRemoved') {
            toast.error('That folder was only partly removed', {
              description:
                'Some files went before a program stopped the rest. Close anything using it, then try again.',
              action: { label: 'Try again', onClick: () => runRemove() },
            })
          }
        },
      }
    )
  }

  /** Repair asks where the folder went; git cannot guess. */
  async function runRepair() {
    const picked = await openFolderDialog({
      directory: true,
      multiple: false,
      title: `Where did the ${worktree.folder_name} folder go?`,
    })
    if (typeof picked !== 'string') return
    m.repairWorktree.mutate(picked)
  }

  const copy = removeConfirmCopy(worktree, dirt)
  const clean = dirt.modified === 0 && dirt.untracked === 0

  const row = (
    <div
      onDoubleClick={() => void openAsTab()}
      style={{ paddingLeft: 24 }}
      className="flex items-center gap-1.5 py-0.5 pr-3 hover:bg-panel2"
    >
      <FolderGit2
        size={11}
        className={cn(
          'flex-none',
          broken
            ? 'text-[var(--gw-amber)]'
            : worktree.is_current
              ? 'text-[var(--gw-accent-text)]'
              : 'text-muted-foreground'
        )}
      />
      <span
        className={cn(
          'truncate text-2xs',
          worktree.is_current ? 'text-foreground' : 'text-sub'
        )}
      >
        {worktree.folder_name}
      </span>
      {worktree.is_run_worktree && (
        <Sparkles size={9} className="flex-none text-muted-foreground" />
      )}
      <span className="ml-auto flex flex-none items-center gap-1.5 pl-1.5 font-mono text-2xs">
        {label && <span className={label.tone}>{label.text}</span>}
        <span className="text-muted-foreground">
          {worktree.branch ?? worktree.head_sha ?? ''}
        </span>
      </span>
    </div>
  )

  const tooltip = broken
    ? (brokenExplanation(worktree.state) ?? '')
    : worktree.is_main
      ? `The project's main checkout${worktree.branch ? `, on ${worktree.branch}` : ''}. Double-click to open it.`
      : `A worktree${worktree.branch ? ` on ${worktree.branch}` : ''} — this project checked out in its own folder. Double-click to open it in its own tab.`

  return (
    <>
      <ContextMenu>
        {/* Outside the trigger: `asChild` merges onto its immediate child, so a
            tooltip nested within would swallow the context-menu handlers. */}
        <TooltipHint label={tooltip}>
          <ContextMenuTrigger asChild>{row}</ContextMenuTrigger>
        </TooltipHint>
        <ContextMenuContent className="w-64">
          <ContextMenuLabel className="overflow-hidden text-ellipsis whitespace-nowrap font-mono text-2xs text-sub">
            {worktree.path}
          </ContextMenuLabel>
          <ContextMenuSeparator />

          {!broken && (
            <PendingMenuItem
              icon={<FolderOpen />}
              label={worktree.is_current ? 'Already open' : 'Open in its own tab'}
              pendingLabel="Opening…"
              pending={opening}
              disabled={opening || worktree.is_current}
              onRun={() => void openAsTab()}
            />
          )}

          {action === 'prune' && (
            <PendingMenuItem
              icon={<Trash2 />}
              label="Tidy up the leftover reference"
              pendingLabel="Tidying…"
              pending={m.pruneWorktrees.isPending}
              disabled={busy}
              onRun={() => m.pruneWorktrees.mutate()}
            />
          )}

          {action === 'repair' && (
            <PendingMenuItem
              icon={<Wrench />}
              label="Point the project at the new location"
              pendingLabel="Repairing…"
              pending={m.repairWorktree.isPending}
              disabled={busy}
              onRun={() => void runRepair()}
            />
          )}

          {/* The main checkout is the project itself; removing it here would be
              removing the thing the user is working in. */}
          {!worktree.is_main && worktree.state === 'ok' && (
            <>
              <ContextMenuSeparator />
              <ContextMenuItem
                variant="destructive"
                disabled={busy || worktree.is_current}
                onSelect={() => void startRemove()}
              >
                <Trash2 />
                {worktree.is_current
                  ? 'Open in this tab — switch away first'
                  : 'Remove this worktree'}
              </ContextMenuItem>
            </>
          )}
        </ContextMenuContent>
      </ContextMenu>

      <ConfirmDialog
        open={confirmRemove}
        onOpenChange={setConfirmRemove}
        destructive
        title={copy.title}
        description={
          <>
            {copy.body} {copy.branchNote}
            {hasUnrecoverableWork(dirt) && !discard && (
              <span className="mt-2 block text-[var(--gw-amber)]">
                Keeping saves them where you can get them back.
              </span>
            )}
          </>
        }
        extra={
          clean ? undefined : (
            <label className="flex cursor-pointer items-center gap-2 text-xs text-sub">
              <input
                type="checkbox"
                checked={discard}
                onChange={(e) => setDiscard(e.target.checked)}
                className="size-3.5 accent-[var(--gw-accent)]"
              />
              Throw the unsaved work away instead of keeping it
            </label>
          )
        }
        confirmLabel={
          clean ? 'Remove worktree' : discard ? 'Throw away and remove' : 'Keep my work and remove'
        }
        pending={m.removeWorktree.isPending}
        pendingLabel="Removing…"
        onConfirm={runRemove}
      />

      <ConfirmDialog
        open={cleanupBranch != null}
        onOpenChange={(o) => !o && setCleanupBranch(null)}
        title={`Delete the ${cleanupBranch} branch too?`}
        description={`Its work is already saved elsewhere, so nothing is lost. Branch lists fill up fast with names nobody needs any more.`}
        confirmLabel="Delete the branch"
        pending={m.deleteBranch.isPending}
        pendingLabel="Deleting…"
        onConfirm={() => {
          if (cleanupBranch) m.deleteBranch.mutate(cleanupBranch)
          setCleanupBranch(null)
        }}
      />
    </>
  )
}

/**
 * The folders this project is checked out into.
 *
 * Three visibility states, not two. The setting governs whether GitWyrm offers
 * to *create* worktrees; a worktree that already exists is part of the
 * repository's real state and is always listed, because a checkout the user
 * cannot see is one they cannot open, remove, or repair when it goes wrong. So:
 * setting off and none exist -> nothing at all; setting off and some exist ->
 * the list without Add, saying why; setting on -> everything.
 */
export function WorktreesSection() {
  const repo = useActiveRepo()
  const { data } = useWorktrees(repo?.id ?? null)
  const m = useGitMutations(repo?.id ?? null)
  const settingOn = useWorkspaceStore((s) => s.enableWorktrees)
  const open = useUiStore((s) => s.sectionOpen.worktrees)
  const toggleSection = useUiStore((s) => s.toggleSection)
  const openModal = useUiStore((s) => s.openModal)

  const list = data ?? []
  const linked = linkedWorktrees(list)
  const rows = visibleWorktrees(list)
  const { show, canAdd, explainNoAdd } = sectionVisibility(settingOn, linked.length)
  const movedRepo = looksLikeMovedRepository(list)

  // No rows is a real state now, not just an unanswered query, so the query's
  // own signal is what says whether there is anything to draw yet.
  if (!show || data == null) return null

  return (
    <div className="group/section">
      <div
        onClick={() => toggleSection('worktrees')}
        className="flex cursor-pointer select-none items-center gap-1.5 py-1.5 pl-2.5 pr-3 hover:bg-panel2"
      >
        <ChevronRight
          size={12}
          strokeWidth={2.4}
          className={cn(
            'flex-none text-muted-foreground transition-transform duration-100',
            open && 'rotate-90'
          )}
        />
        <span className="text-2xs font-bold tracking-[.09em] text-sub">WORKTREES</span>

        {canAdd && (
          <TooltipButton
            onClick={(e) => {
              e.stopPropagation()
              openModal('addWorktree')
            }}
            tooltip="Add a worktree — another branch checked out in its own folder"
            className="ml-auto flex size-4 flex-none items-center justify-center rounded text-muted-foreground opacity-0 hover:bg-panel3 hover:text-foreground focus:opacity-100 group-hover/section:opacity-100"
          >
            <Plus size={12} strokeWidth={2.4} />
          </TooltipButton>
        )}

        <span className={cn('font-mono text-2xs text-muted-foreground', canAdd ? 'ml-1.5' : 'ml-auto')}>
          {rows.length}
        </span>
      </div>

      {open && (
        <div className="pb-1">
          {/* Every linked worktree broken at once means the project folder
              itself moved. One repair beats a list of rows that all say the
              same thing without saying why. */}
          {movedRepo && (
            <div className="mx-2.5 mb-1 rounded border border-[var(--gw-amber)]/30 bg-[var(--gw-amber)]/10 px-2 py-1.5">
              <div className="flex items-start gap-1.5">
                <AlertTriangle size={11} className="mt-px flex-none text-[var(--gw-amber)]" />
                <div className="text-2xs leading-relaxed text-sub">
                  Moving this project broke the link to every one of its worktrees. Your files are
                  all still there.
                  <button
                    onClick={() => m.repairWorktree.mutate(null)}
                    disabled={m.repairWorktree.isPending}
                    className="mt-1 block rounded bg-panel3 px-1.5 py-0.5 text-2xs text-foreground hover:bg-panel2 disabled:opacity-50"
                  >
                    {m.repairWorktree.isPending ? 'Fixing…' : 'Fix them all'}
                  </button>
                </div>
              </div>
            </div>
          )}

          {rows.map((w) => (
            <WorktreeRow key={w.path} worktree={w} canAdd={canAdd} />
          ))}

          {/* Where the term gets taught. Someone who has never heard "worktree"
              meets it here, so the line names it and then says what it is --
              rather than renaming the concept to something friendlier, which
              would leave them unable to search for help or follow along with
              anything else that uses the real word. Once that has done its job,
              the tips switch turns it off. */}
          {canAdd && linked.length === 0 && (
            <SidebarTip>
              A worktree is another branch of this project checked out in its own folder, so you can
              work on two at once without putting anything aside.
            </SidebarTip>
          )}

          {explainNoAdd && (
            <div style={{ paddingLeft: 24 }} className="pr-3 pt-1 text-2xs leading-relaxed text-muted-foreground">
              Adding worktrees is turned off in Settings. The ones here still work.
            </div>
          )}
        </div>
      )}
    </div>
  )
}
