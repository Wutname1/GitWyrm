import { useState } from 'react'
import {
  ArrowUpCircle,
  ChevronRight,
  Download,
  ExternalLink,
  Package,
  Plus,
  RotateCcw,
  Trash2,
} from 'lucide-react'
import { cn } from '@/lib/utils'
import type { SubmoduleStatus } from '@/lib/bindings'
import { plural } from '@/lib/gitDisplay'
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
import { TooltipButton } from '@/components/ui/tooltip'
import { useGitMutations } from '@/hooks/useGitMutations'
import { useSubmodules } from '@/hooks/useGitQueries'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo } from '@/stores/workspaceStore'
import { openWebUrl, remoteWebTarget } from '@/lib/remoteWeb'

/** Short label for the row's right edge, and the colour that goes with it. */
function stateLabel(s: SubmoduleStatus): { text: string; tone: string } | null {
  if (s.state === 'uninitialized') {
    return { text: 'not downloaded', tone: 'text-muted-foreground' }
  }
  if (s.state === 'in_sync') return null
  // Moved: say which way it went, since ahead and behind mean different things.
  if (s.ahead > 0 && s.behind > 0) return { text: 'changed', tone: 'text-[var(--gw-amber)]' }
  if (s.ahead > 0) return { text: `${s.ahead} ahead`, tone: 'text-[var(--gw-amber)]' }
  if (s.behind > 0) return { text: `${s.behind} behind`, tone: 'text-[var(--gw-amber)]' }
  return { text: 'changed', tone: 'text-[var(--gw-amber)]' }
}

/** Plain-language explanation of the row, shown on hover. */
function tooltip(s: SubmoduleStatus): string {
  const follows = s.branch ? ` Follows ${s.branch}.` : ''
  if (s.state === 'uninitialized') {
    return `Not downloaded yet, so this folder is empty. Download it to get the files.${follows}`
  }
  if (s.state === 'in_sync') {
    return `Up to date with the version this project expects.${follows}`
  }
  if (s.ahead > 0 && s.behind > 0) {
    return `This is on a different version than the project expects.${follows}`
  }
  if (s.ahead > 0) {
    return `${plural(s.ahead, 'newer commit')} than the project expects. Commit to save this update.${follows}`
  }
  return `${plural(s.behind, 'commit')} older than the project expects.${follows}`
}

function SubmoduleRow({ sub }: { sub: SubmoduleStatus }) {
  const repo = useActiveRepo()
  const m = useGitMutations(repo?.id ?? null)
  const [confirmReset, setConfirmReset] = useState(false)
  const [confirmRemove, setConfirmRemove] = useState(false)
  const [deleteFiles, setDeleteFiles] = useState(true)

  const name = sub.path.split('/').pop() ?? sub.path
  const label = stateLabel(sub)
  const uninitialized = sub.state === 'uninitialized'
  // Only offer a web link for a URL that resolves to a browsable page. The
  // shared parser handles self-hosted and Azure routes too.
  const webTarget = sub.url ? remoteWebTarget(sub.url) : null
  const busy =
    m.bumpSubmodule.isPending || m.updateSubmodule.isPending || m.removeSubmodule.isPending

  const row = (
    <div
      style={{ paddingLeft: 24 }}
      className="flex items-center gap-1.5 py-0.5 pr-3 hover:bg-panel2"
      title={tooltip(sub)}
    >
      <Package
        size={11}
        className={cn(
          'flex-none',
          uninitialized
            ? 'text-muted-foreground'
            : sub.state === 'moved'
              ? 'text-[var(--gw-amber)]'
              : 'text-muted-foreground'
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
      <span className="ml-auto flex flex-none items-center gap-1.5 pl-1.5 font-mono text-2xs">
        {label && <span className={label.tone}>{label.text}</span>}
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
        <ContextMenuTrigger asChild>{row}</ContextMenuTrigger>
        <ContextMenuContent className="w-60">
          <ContextMenuLabel className="overflow-hidden text-ellipsis whitespace-nowrap font-mono text-2xs text-sub">
            {sub.path}
          </ContextMenuLabel>
          <ContextMenuSeparator />

          {uninitialized ? (
            <PendingMenuItem
              icon={<Download />}
              label="Download it"
              pendingLabel="Downloading…"
              pending={m.updateSubmodule.isPending}
              disabled={busy}
              onRun={() => m.updateSubmodule.mutate({ path: sub.path, init: true })}
            />
          ) : (
            <PendingMenuItem
              icon={<ArrowUpCircle />}
              label="Update to latest"
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

          {sub.state === 'moved' && (
            <>
              <ContextMenuSeparator />
              <ContextMenuItem variant="destructive" onSelect={() => setConfirmReset(true)}>
                <RotateCcw />
                Undo my changes
              </ContextMenuItem>
            </>
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
        title={`Undo your changes to ${name}?`}
        description={
          <>
            This puts <span className="font-mono text-foreground">{sub.path}</span> back to the
            version this project expects, dropping the version it points at now. This can't be
            undone.
          </>
        }
        confirmLabel="Undo changes"
        pending={m.updateSubmodule.isPending}
        pendingLabel="Undoing…"
        onConfirm={() => m.updateSubmodule.mutate({ path: sub.path, init: false })}
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

        <TooltipButton
          onClick={(e) => {
            e.stopPropagation()
            openModal('addSubmodule')
          }}
          tooltip="Add a submodule"
          className={cn(
            'flex size-4 flex-none items-center justify-center rounded text-muted-foreground opacity-0 hover:bg-panel3 hover:text-foreground focus:opacity-100 group-hover/section:opacity-100',
            missing === 0 && 'ml-auto'
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
