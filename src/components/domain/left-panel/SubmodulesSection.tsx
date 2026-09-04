import { useState } from 'react'
import {
  ArrowUpCircle,
  ChevronRight,
  Download,
  ExternalLink,
  FolderOpen,
  Package,
  Plus,
  RotateCcw,
  Trash2,
} from 'lucide-react'
import { toast } from 'sonner'
import { cn } from '@/lib/utils'
import { commands, type SubmoduleStatus } from '@/lib/bindings'
import { unwrap } from '@/lib/queryKeys'
import { plural } from '@/lib/gitDisplay'
import { describeHead, isDetached, readSubmodule } from '@/lib/submoduleState'
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
import { useGitMutations } from '@/hooks/useGitMutations'
import { useSubmodules } from '@/hooks/useGitQueries'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo } from '@/stores/workspaceStore'
import { useOpenSubmoduleRepo } from '@/hooks/useRepoActions'
import { openWebUrl, remoteWebTarget } from '@/lib/remoteWeb'

/**
 * What the row says on hover: the situation, where the folder sits, and how to
 * open it. `readSubmodule` owns the wording so the row, the menu and the
 * confirmation cannot describe the same state three different ways.
 */
function tooltip(sub: SubmoduleStatus): string {
  const { meaning } = readSubmodule(sub)
  const follows = sub.branch ? ` Follows ${sub.branch}.` : ''
  const where = sub.state === 'uninitialized' ? '' : ` ${describeHead(sub)}.`
  // Double-click is otherwise invisible, so every row advertises it.
  return `${meaning}${where}${follows} Double-click to open it in a tab under this project.`
}

function SubmoduleRow({ sub }: { sub: SubmoduleStatus }) {
  const repo = useActiveRepo()
  const m = useGitMutations(repo?.id ?? null)
  const openRepo = useOpenSubmoduleRepo()
  const [confirmReset, setConfirmReset] = useState(false)
  const [confirmRemove, setConfirmRemove] = useState(false)
  const [deleteFiles, setDeleteFiles] = useState(true)
  const [opening, setOpening] = useState(false)

  const name = sub.path.split('/').pop() ?? sub.path
  const reading = readSubmodule(sub)
  const detached = isDetached(sub)
  const uninitialized = sub.state === 'uninitialized'
  // Only offer a web link for a URL that resolves to a browsable page. The
  // shared parser handles self-hosted and Azure routes too.
  const webTarget = sub.url ? remoteWebTarget(sub.url) : null
  const busy =
    m.bumpSubmodule.isPending || m.updateSubmodule.isPending || m.removeSubmodule.isPending

  /**
   * Opens the submodule's own checkout as a tab nested under this project, so it
   * can be branched and committed while still reading as part of what it belongs
   * to. A submodule that was never downloaded has an empty folder and nothing to
   * open, so fetch it first rather than making the user run two separate steps to
   * get to the same place.
   */
  async function openAsRepo() {
    if (!repo || opening) return
    setOpening(true)
    try {
      if (uninitialized) {
        await m.updateSubmodule.mutateAsync({ path: sub.path, init: true })
      }
      // Ask the backend where the submodule actually lives instead of joining
      // the path ourselves: repository discovery searches upward, so a folder
      // that failed to check out would quietly open the parent project again.
      // This is the only step here without its own failure toast.
      const workdir = await commands
        .submoduleWorkdir(repo.id, sub.path)
        .then(unwrap)
        .catch((e: Error) => {
          toast.error(e.message)
          throw e
        })
      await openRepo.mutateAsync({ path: workdir, parentPath: repo.path })
    } catch {
      // Every step above reports its own failure.
    } finally {
      setOpening(false)
    }
  }

  const row = (
    <div
      onDoubleClick={() => void openAsRepo()}
      style={{ paddingLeft: 24 }}
      className="flex items-center gap-1.5 py-0.5 pr-3 hover:bg-panel2"
    >
      <Package
        size={11}
        className={cn(
          'flex-none',
          sub.state === 'moved' ? 'text-[var(--gw-amber)]' : 'text-muted-foreground'
        )}
      />
      <span
        className={cn(
          'truncate text-2xs',
          sub.state === 'in_sync' ? 'text-sub' : 'text-foreground'
        )}
      >
        {name}
      </span>
      {/* Where the folder sits. A checkout on no branch is the ordinary state
          after an update, but a commit made there belongs to no branch -- so it
          is named rather than left to infer from a sha. */}
      {detached ? (
        <span className="flex-none rounded-sm bg-panel3 px-1 text-[9px] leading-4 text-muted-foreground">
          no branch
        </span>
      ) : sub.head_branch ? (
        <span className="min-w-0 flex-none truncate text-[9px] text-muted-foreground">
          {sub.head_branch}
        </span>
      ) : null}
      <span className="ml-auto flex flex-none items-center gap-1.5 pl-1.5 font-mono text-2xs">
        {reading.label && <span className={reading.tone}>{reading.label}</span>}
        {/* Where it actually sits, which only differs from the recorded commit
            when it has moved -- that is the case worth showing the sha for. */}
        <span className="text-muted-foreground">
          {(sub.workdir_sha ?? sub.recorded_sha).slice(0, 7)}
        </span>
      </span>
    </div>
  )

  return (
    <>
      <ContextMenu>
        {/* Outside the trigger, not inside the row: `asChild` merges onto its
            immediate child, so a tooltip nested within would swallow the
            context-menu handlers. */}
        <TooltipHint label={tooltip(sub)}>
          <ContextMenuTrigger asChild>{row}</ContextMenuTrigger>
        </TooltipHint>
        <ContextMenuContent className="w-60">
          <ContextMenuLabel className="overflow-hidden text-ellipsis whitespace-nowrap font-mono text-2xs text-sub">
            {sub.path}
          </ContextMenuLabel>
          <ContextMenuSeparator />

          <PendingMenuItem
            icon={<FolderOpen />}
            label="Open in its own tab"
            pendingLabel={uninitialized ? 'Downloading…' : 'Opening…'}
            pending={opening}
            disabled={busy || opening}
            onRun={() => void openAsRepo()}
          />

          <ContextMenuSeparator />

          {/* The two intents, named for what the user wants rather than for
              the git command behind them. "Match" is `git submodule update`;
              "newest" is `git submodule update --remote`. Both were already
              here -- one was buried under "Undo my changes", which blamed the
              user for a state a pull had left. */}
          {reading.canMatchProject && (
            <PendingMenuItem
              icon={uninitialized ? <Download /> : <RotateCcw />}
              label={
                uninitialized ? 'Download it' : 'Match the version this project expects'
              }
              pendingLabel={uninitialized ? 'Downloading…' : 'Matching…'}
              pending={m.updateSubmodule.isPending}
              disabled={busy}
              onRun={() => {
                // Only confirm when something of the user's would go. Catching
                // up a folder someone else moved discards nothing.
                if (reading.matchIsSafe) {
                  m.updateSubmodule.mutate({ path: sub.path, init: true })
                } else {
                  setConfirmReset(true)
                }
              }}
            />
          )}

          {!uninitialized && (
            <PendingMenuItem
              icon={<ArrowUpCircle />}
              label="Move to the newest version"
              pendingLabel="Updating…"
              pending={m.bumpSubmodule.isPending}
              disabled={busy}
              onRun={() => m.bumpSubmodule.mutate(sub.path)}
            />
          )}

          {webTarget && (
            <ContextMenuItem
              onSelect={() => openWebUrl(webTarget.repositoryUrl, webTarget.label)}
            >
              <ExternalLink />
              View on {webTarget.label}
            </ContextMenuItem>
          )}

          <ContextMenuSeparator />
          <ContextMenuItem variant="destructive" onSelect={() => setConfirmRemove(true)}>
            <Trash2 />
            Remove from project
          </ContextMenuItem>
        </ContextMenuContent>
      </ContextMenu>

      <ConfirmDialog
        open={confirmReset}
        onOpenChange={setConfirmReset}
        destructive
        title={`Match the version this project expects for ${name}?`}
        description={
          <>
            <span className="font-mono text-foreground">{sub.path}</span> is on a
            {reading.situation === 'diverged' ? ' different line of work' : ' newer version'}{' '}
            than this project expects. Matching the project drops
            {reading.situation === 'diverged'
              ? ' what is only in this folder'
              : ` the ${plural(sub.ahead, 'newer commit')} here`}
            , which can't be undone. Commit inside the folder first to keep it.
          </>
        }
        confirmLabel="Match the project"
        pending={m.updateSubmodule.isPending}
        pendingLabel="Matching…"
        onConfirm={() => m.updateSubmodule.mutate({ path: sub.path, init: true })}
      />

      <ConfirmDialog
        open={confirmRemove}
        onOpenChange={setConfirmRemove}
        destructive
        title={`Remove ${name} from this project?`}
        description={
          <>
            This project will stop including{' '}
            <span className="font-mono text-foreground">{sub.path}</span>. The removal is added to
            your changes, so it isn't final until you commit.
          </>
        }
        extra={
          <label className="flex cursor-pointer items-center gap-2 text-xs text-sub">
            <input
              type="checkbox"
              checked={deleteFiles}
              onChange={(e) => setDeleteFiles(e.target.checked)}
              className="size-3.5 accent-[var(--gw-accent)]"
            />
            Also delete the folder from my computer
          </label>
        }
        confirmLabel="Remove submodule"
        pending={m.removeSubmodule.isPending}
        pendingLabel="Removing…"
        onConfirm={() => m.removeSubmodule.mutate({ path: sub.path, deleteFiles })}
      />
    </>
  )
}

/**
 * The submodules a project includes, with what each one points at.
 *
 * Hidden entirely when the repo has none -- most projects have no submodules,
 * and an always-present empty section is noise for them. Projects that do use
 * them need the list visible, not only when something has gone wrong.
 */
export function SubmodulesSection() {
  const repo = useActiveRepo()
  const { data: submodules } = useSubmodules(repo?.id ?? null)
  const m = useGitMutations(repo?.id ?? null)
  const open = useUiStore((s) => s.sectionOpen.submodules)
  const toggleSection = useUiStore((s) => s.toggleSection)
  const openModal = useUiStore((s) => s.openModal)

  const list = submodules ?? []
  const missing = list.filter((s) => s.state === 'uninitialized').length
  // Folders left behind by someone else's update. Separated from the ones
  // holding work of their own, because only these can be caught up without a
  // decision -- and this is the state that reads as broken from the sidebar.
  const behind = list.filter((s) => readSubmodule(s).situation === 'behind-project')

  if (list.length === 0) return null

  return (
    <div className="group/section">
      <div
        onClick={() => toggleSection('submodules')}
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
        <span className="text-2xs font-bold tracking-[.09em] text-sub">SUBMODULES</span>

        {missing > 0 && (
          <TooltipButton
            onClick={(e) => {
              e.stopPropagation()
              m.initAllSubmodules.mutate()
            }}
            disabled={m.initAllSubmodules.isPending}
            tooltip={`Download ${plural(missing, 'submodule')} that ${missing === 1 ? "isn't" : "aren't"} on your computer yet`}
            className="ml-auto flex flex-none items-center gap-1 rounded-sm bg-[var(--gw-amber)]/15 px-1 font-mono text-2xs text-[var(--gw-amber)] hover:bg-[var(--gw-amber)]/25"
          >
            <Download size={9} />
            {missing}
          </TooltipButton>
        )}

        {behind.length > 0 && (
          <TooltipButton
            onClick={(e) => {
              e.stopPropagation()
              for (const s of behind) {
                m.updateSubmodule.mutate({ path: s.path, init: true })
              }
            }}
            disabled={m.updateSubmodule.isPending}
            tooltip={`Catch up ${plural(behind.length, 'folder')} that ${behind.length === 1 ? 'has' : 'have'} not reached the version this project expects. Nothing of yours is lost.`}
            className={cn(
              'flex flex-none items-center gap-1 rounded-sm bg-[var(--gw-amber)]/15 px-1 font-mono text-2xs text-[var(--gw-amber)] hover:bg-[var(--gw-amber)]/25',
              missing === 0 && 'ml-auto'
            )}
          >
            <RotateCcw size={9} />
            {behind.length}
          </TooltipButton>
        )}

        <TooltipButton
          onClick={(e) => {
            e.stopPropagation()
            openModal('addSubmodule')
          }}
          tooltip="Add a submodule"
          className={cn(
            'flex size-4 flex-none items-center justify-center rounded text-muted-foreground opacity-0 hover:bg-panel3 hover:text-foreground focus:opacity-100 group-hover/section:opacity-100',
            missing === 0 && behind.length === 0 && 'ml-auto'
          )}
        >
          <Plus size={12} strokeWidth={2.4} />
        </TooltipButton>

        <span className="ml-1.5 font-mono text-2xs text-muted-foreground">{list.length}</span>
      </div>

      {open && (
        <div className="pb-1">
          {list.map((s) => (
            <SubmoduleRow key={s.path} sub={s} />
          ))}
        </div>
      )}
    </div>
  )
}
