import { useEffect, useRef, useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Clock3, FileDiff, Folders, ListFilter, PanelLeftClose, PanelLeftOpen, Search, X } from 'lucide-react'
import { commands, type SessionListFilterInput } from '@/lib/bindings'
import { unwrap } from '@/lib/queryKeys'
import { summarizeDiskUsage } from '@/lib/agentDeskResult'
import { useAgentSessionHeaders } from '@/hooks/useAgentSessions'
import { ConfirmDialog } from '@/components/modals/ConfirmDialog'
import { useAgentSessionMutations } from '@/hooks/useAgentSessionMutations'
import { NewSessionButton } from '@/components/domain/agent-desk/NewSessionButton'
import { SessionGroups } from '@/components/domain/agent-desk/SessionGroups'
import { resolveSessionRepoFilter, type SidebarGroupMode } from '@/lib/agentSessionGrouping'
import { useAgentDeskUiStore } from '@/stores/agentDeskUiStore'
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { cn } from '@/lib/utils'

/** Below this width the sidebar becomes a drawer instead of a fixed column (design.md's "narrow widths" clause, tasks.md 3.1). */
const DRAWER_BREAKPOINT_PX = 620
/** design.md: "Left column: 218-282 px". */
const SIDEBAR_MIN_PX = 218
const SIDEBAR_MAX_PX = 282

/**
 * The mockup's 900px rule, which narrows the chat list to 176px rather than
 * dropping it (`.ag-shell { grid-template-columns: 176px ... }`).
 *
 * design.md's "218-282 px" describes the column at a comfortable width. It
 * was being applied at every width above the drawer breakpoint, including the
 * band between 620 and 900 -- so at the window's own 720px minimum a 218px
 * list sat beside a conversation with barely 440px left. The drawer could
 * never rescue it, because 620 is below the smallest size the window allows.
 */
const SIDEBAR_COMPACT_PX = 176
const SIDEBAR_COMPACT_BELOW_PX = 900

interface SessionSidebarProps {
  /**
   * The main window's current repo, i.e. what "this project" means for the
   * "This project only" toggle below (R4.1) -- `null` while that repo is
   * still opening. Deliberately *not* the identity of the list: the list
   * itself is app-wide by default (`resolveSessionRepoFilter`), so a chat
   * from any project keeps showing here even before this resolves, and
   * switching the main window's repo does not clear the list or the
   * selection (R4.2).
   */
  currentRepoId: string | null
  /** Shown on the "This project only" toggle so the user knows what it scopes to. */
  currentRepoName: string | null
  selectedId: string | null
  /**
   * Selection is reported upward, never applied locally -- per the seam
   * contract, the workspace-layout package (not this sidebar) decides which
   * pane receives it. This component only ever calls this prop; it never
   * writes to a "current session" store of its own.
   */
  onSelectSession: (sessionId: string) => void
  /** Also reported upward; the workspace-layout package owns session creation (architecture.md section 8's kickoff pipeline). */
  onNewSession: () => void
  /** Extra filter beyond repoId, e.g. archived visibility. Defaults to active, non-archived sessions. */
  filter?: Partial<SessionListFilterInput>
}

/**
 * The Agent Desk left column (tasks 3.1-3.7): virtualized one-line session
 * rows with Recent/Project/Diff grouping, New chat, rename/archive, and a
 * narrow-width drawer.
 *
 * Ownership boundary: this component and everything it renders decide *how*
 * sessions are listed and grouped. It never decides *what happens* when one
 * is picked or created -- both go out through injected callbacks so the
 * workspace-layout package (a different cluster) can route them to whichever
 * pane is active, per the shell-scaffold brief's seam contract.
 *
 * R4.1: lists sessions app-wide, grouped by project, by default -- the
 * repository is an optional filter the user can turn on ("This project
 * only"), not the identity of this window. The toggle defaults to off and is
 * *not* persisted, matching this component's other view-only state (grouping
 * mode, collapsed groups): the point of an app-wide Desk is that reopening it
 * shows every project's chats again, not whatever repo happened to be
 * current the last time someone scoped the list down.
 */
export function SessionSidebar({
  currentRepoId,
  currentRepoName,
  selectedId,
  onSelectSession,
  onNewSession,
  filter,
}: SessionSidebarProps) {
  const [scopeToCurrentRepo, setScopeToCurrentRepo] = useState(false)
  const effectiveRepoId = resolveSessionRepoFilter(scopeToCurrentRepo, currentRepoId)
  // Finding one chat among many. The backend has matched titles since the
  // list command shipped (`SessionListFilter::title_contains`, with its own
  // test) and the frontend passed `null` for it everywhere, so someone who
  // starts dozens of chats a day had only scrolling.
  // Whether the list is showing active chats or archived ones.
  //
  // Archive was a one-way trapdoor: `archived: false` was hardcoded, no caller
  // ever passed the `filter` prop that would change it, and the archive toast
  // promised "You can restore it from the Archived filter any time" -- a
  // filter that did not exist. The Restore action on the row was live code no
  // one could reach, because reaching it needed a row the query could never
  // return. Worse, the delete dialog recommends Archive as the safe option.
  const [showArchived, setShowArchived] = useState(false)
  // Persisted: someone who works by project should not re-pick it on every
  // launch (architecture.md lists sidebar grouping among agentDeskUiStore's state).
  const grouping = useAgentDeskUiStore((s) => s.layout.sidebarGrouping)
  const setGrouping = useAgentDeskUiStore((s) => s.setSidebarGrouping)
  const [search, setSearch] = useState('')
  const [debouncedSearch, setDebouncedSearch] = useState('')
  useEffect(() => {
    // Typing re-queries the list, so wait for a pause rather than firing a
    // request per keystroke.
    const id = setTimeout(() => setDebouncedSearch(search.trim()), 180)
    return () => clearTimeout(id)
  }, [search])
  // `isError` was never read, so a list that could not be fetched showed the
  // same words as one that is genuinely empty -- telling someone with chats
  // that they had none.
  const { headers, isLoading, isError, diagnostics, hasNextPage, isFetchingNextPage, fetchNextPage } = useAgentSessionHeaders({
    repoId: effectiveRepoId,
    projectPath: null,
    states: [],
    sourceKinds: [],
    hasChangedFiles: null,
    archived: showArchived,
    titleContains: debouncedSearch === '' ? null : debouncedSearch,
    ...filter,
  })
  const { rename, archive, markRead, remove } = useAgentSessionMutations()
  // Which chat a delete has been asked for, held until it is confirmed.
  const [pendingDelete, setPendingDelete] = useState<{ id: string; title: string } | null>(null)
  // Copies this chat is holding, so the confirmation can say what deleting
  // strands. Fetched only while a delete is pending.
  const copiesOnDisk = useQuery({
    queryKey: ['agentCopiesOnDisk'],
    queryFn: async () => unwrap(await commands.agentResultCopiesOnDisk()),
    enabled: pendingDelete != null,
  })
  const pendingDeleteCopies = (copiesOnDisk.data ?? []).filter((c) => c.sessionId === pendingDelete?.id)
  // A failed read is not "no copies". Saying nothing here would be the same
  // absence-for-a-failure inversion the guard test exists to catch -- and it
  // did catch this, on the dialog where the cost is a stranded folder.
  //
  // `isPending` counts too, and that was missed: the query STARTS when this
  // dialog opens, so for its whole in-flight window `data` was undefined,
  // the filtered list was empty, `isError` was false, and the dialog showed
  // no warning at all -- reading as a confident "no working copy" at the one
  // moment the app had not looked yet. Not-yet-known is a kind of unknown.
  //
  // Deliberately NOT gating the Delete button on this. `agentResultCopiesOnDisk`
  // walks every run worktree recursively to total its size, which on a tree
  // with node_modules is seconds, and every other confirm dialog in this app
  // gates only on its own mutation, never on a background read. Blocking here
  // would break the rule that an action responds immediately, at the instant
  // the person acted. Telling them what is and is not known is the honest
  // move; deciding for them is not.
  const copiesUnknown = copiesOnDisk.isError || copiesOnDisk.isPending

  // Narrow-width drawer (task: "a hidden sidebar with no reopen affordance
  // violates house Rule #1"). Tracks the *container's* width via
  // ResizeObserver -- matching the pattern RepositoryTabs.tsx already uses
  // for its own responsive behavior -- rather than a window-level media
  // query, since the sidebar can be narrow inside a wide window (e.g. Split
  // View with a docked right panel) and vice versa.
  const containerRef = useRef<HTMLDivElement>(null)
  const [drawerOpen, setDrawerOpen] = useState(false)

  // Escape closes the drawer wherever focus happens to be. This used to sit as
  // `onKeyDown` on the drawer's own div, which is not focusable -- so the key
  // only worked if focus had already landed inside, and the comment promising
  // "keyboard users close via the toggle or Escape" was true only sometimes.
  useEffect(() => {
    if (!drawerOpen) return
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setDrawerOpen(false)
    }
    document.addEventListener('keydown', onKey)
    return () => document.removeEventListener('keydown', onKey)
  }, [drawerOpen])


  // Measure the WINDOW, not this element.
  //
  // This used to observe `containerRef` -- the sidebar's own wrapper -- which
  // is only ever 218-282px wide, i.e. permanently under the breakpoint. So the
  // sidebar collapsed to a drawer on every window size and never came back:
  // entering drawer mode shrinks the wrapper to a 36px strip, which keeps the
  // measurement below the threshold forever. Only the viewport can answer
  // "is there room for a sidebar here".
  const [viewportWidth, setViewportWidth] = useState<number>(() =>
    typeof window === 'undefined' ? DRAWER_BREAKPOINT_PX + 1 : window.innerWidth
  )

  useEffect(() => {
    const onResize = () => setViewportWidth(window.innerWidth)
    onResize()
    window.addEventListener('resize', onResize)
    return () => window.removeEventListener('resize', onResize)
  }, [])

  const isDrawerMode = viewportWidth < DRAWER_BREAKPOINT_PX
  // Between the drawer breakpoint and 900 the list stays, narrower.
  const isCompactColumn = !isDrawerMode && viewportWidth < SIDEBAR_COMPACT_BELOW_PX

  // Selecting a session while the sidebar is a drawer implies the user is
  // done with it -- closing automatically keeps the drawer from covering the
  // conversation it just opened (visible response to the click, per house
  // Rule #1, without a second explicit close step).
  const handleSelect = (sessionId: string) => {
    onSelectSession(sessionId)
    if (isDrawerMode) setDrawerOpen(false)
    const target = headers.find((h) => h.sessionId === sessionId)
    if (target?.unread) markRead.mutate(sessionId)
  }

  const handleNewSession = () => {
    onNewSession()
    if (isDrawerMode) setDrawerOpen(false)
  }

  const body = (
    <div
      className="flex h-full flex-col bg-panel"
      style={
        isDrawerMode
          ? undefined
          : isCompactColumn
            ? { width: SIDEBAR_COMPACT_PX, minWidth: SIDEBAR_COMPACT_PX, maxWidth: SIDEBAR_COMPACT_PX }
            : { minWidth: SIDEBAR_MIN_PX, maxWidth: SIDEBAR_MAX_PX }
      }
    >
      <div className="flex-none p-1.5 pb-1">
        <NewSessionButton onNewSession={handleNewSession} />
      </div>

      <div className="flex flex-none items-center gap-1 px-1.5 pb-1.5">
        <div className="flex min-w-0 flex-1 items-center gap-1 rounded border border-border bg-panel2 px-1.5 focus-within:border-primary">
          <Search size={11} className="flex-none text-muted-foreground" aria-hidden />
          <input
            type="text"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="Find a chat"
            aria-label="Find a chat by name"
            className="min-w-0 flex-1 bg-transparent py-1 text-2xs text-foreground outline-none placeholder:text-muted-foreground"
          />
          {search !== '' && (
            <button
              type="button"
              onClick={() => setSearch('')}
              aria-label="Clear the search"
              className="flex-none rounded p-0.5 text-muted-foreground hover:text-foreground"
            >
              <X size={11} aria-hidden />
            </button>
          )}
        </div>
        <ChatListFilterMenu
          grouping={grouping}
          onGroupingChange={setGrouping}
          showArchived={showArchived}
          onShowArchivedChange={setShowArchived}
          scopeToCurrentRepo={scopeToCurrentRepo}
          onScopeToCurrentRepoChange={setScopeToCurrentRepo}
          currentRepoId={currentRepoId}
          currentRepoName={currentRepoName}
        />
      </div>

      {/* A filter that hides chats has to say so where the list is, or a
          person who switched to archived chats last week reads the list as
          "my chats are gone". */}
      {(showArchived || scopeToCurrentRepo) && (
        <div className="flex flex-none items-center gap-1.5 px-2.5 pb-1 text-2xs text-muted-foreground">
          <span className="min-w-0 truncate">
            {showArchived ? 'Archived chats' : 'Active chats'}
            {scopeToCurrentRepo && currentRepoName ? ` in ${currentRepoName}` : ''}
          </span>
          <button
            type="button"
            onClick={() => {
              setShowArchived(false)
              setScopeToCurrentRepo(false)
            }}
            className="flex-none rounded px-1 text-accent-text hover:bg-panel2"
          >
            Show all
          </button>
        </div>
      )}

      <SessionGroups
        headers={headers}
        failed={isError}
        searchTerm={debouncedSearch === '' ? undefined : debouncedSearch}
        archived={showArchived}
        selectedId={selectedId}
        onSelectSession={handleSelect}
        onRename={(sessionId, title) => rename.mutate({ sessionId, title })}
        onArchive={(sessionId, archived) => archive.mutate({ sessionId, archived })}
        onDelete={(sessionId, title) => setPendingDelete({ id: sessionId, title })}
      />

      <ConfirmDialog
        open={pendingDelete !== null}
        onOpenChange={(open) => {
          if (!open) setPendingDelete(null)
        }}
        title={pendingDelete ? `Delete "${pendingDelete.title}"?` : 'Delete this chat?'}
        description={
          <>
            This chat and everything in it are removed for good. Any files the agent already
            changed stay exactly where they are, and nothing in your project is touched. If you
            only want it out of the way, Archive keeps it.
            {/*
              The working copy is the part this dialog used to leave out. It is
              not removed by deleting, and afterwards nothing can see it: the
              "what GitWyrm holds on disk" screen finds copies by walking chat
              records, and this deletes the record. So the copy becomes
              unreachable rather than merely left behind.

              Said only when there is one, and it names the size, because the
              honest answer to "should I clear this first?" depends on how big
              it is.
            */}
            {copiesUnknown && (
              <span className="mt-2 block font-semibold text-[var(--gw-amber)]">
                {copiesOnDisk.isPending
                  ? 'Still checking whether this chat is holding a working copy on your machine. If it is, deleting the chat now leaves that behind.'
                  : 'GitWyrm could not check whether this chat is holding a working copy on your machine. If it is, deleting the chat leaves that behind.'}
              </span>
            )}
            {pendingDeleteCopies.length > 0 && (
              <span className="mt-2 block font-semibold text-[var(--gw-amber)]">
                This chat still has {summarizeDiskUsage(pendingDeleteCopies).toLowerCase().replace(/\.$/, '')} on
                this machine. Deleting the chat leaves that behind with no way to find it again. Clear it first from
                Agent Setup if you want the space back.
              </span>
            )}
          </>
        }
        confirmLabel="Delete"
        destructive
        // Held open while the delete runs, the way every other destructive
        // dialog here works. It used to fire and close in the same breath, so
        // a second click landing before the close could send the delete
        // twice -- and the second one comes back "failed", because the chat
        // is already gone. That produced a red "Could not delete that chat"
        // for a delete that had worked perfectly.
        pending={remove.isPending}
        pendingLabel="Deleting…"
        keepOpenOnConfirm
        onConfirm={() => {
          if (!pendingDelete) return
          remove.mutate(pendingDelete.id, {
            // Closed on settle rather than on success: a delete that refused
            // because the chat is still working has already said so in its
            // own words, and leaving the dialog up would ask the person to
            // dismiss the same news twice.
            onSettled: () => setPendingDelete(null),
          })
        }}
      />
      {isLoading && headers.length === 0 && (
        <p className="flex-none px-3 py-2 text-2xs text-muted-foreground">Loading chats…</p>
      )}
      {/*
        The list is fetched a hundred at a time and nothing ever asked for the
        next page, so someone with more chats than that simply stopped seeing
        the older ones -- with no count, no notice, and no way to reach them.
        Paging was already supported by the query; only the button was missing.
      */}
      {hasNextPage && (
        <button
          type="button"
          onClick={() => void fetchNextPage()}
          disabled={isFetchingNextPage}
          className="flex-none px-3 py-2 text-left text-2xs font-semibold text-accent-text hover:bg-panel3 disabled:cursor-not-allowed disabled:text-muted-foreground"
        >
          {isFetchingNextPage ? 'Loading older chats…' : 'Show older chats'}
        </button>
      )}
      {/*
        A chat whose file cannot be read used to disappear with no sign at all,
        which reads as lost work rather than as one damaged file. The backend
        has always collected these, with a plain reason each, precisely so this
        could be said out loud -- nothing was reading them.

        It sits below the list, not over it: the other chats are fine and stay
        the main thing on screen.
      */}
      {diagnostics.length > 0 && (
        <div className="flex-none border-t border-border px-3 py-2">
          <p className="text-2xs font-semibold text-[var(--gw-amber)]">
            {diagnostics.length === 1
              ? '1 chat could not be opened.'
              : `${diagnostics.length} chats could not be opened.`}{' '}
            Everything else here is fine.
          </p>
          <ul className="mt-1 flex flex-col gap-0.5">
            {diagnostics.slice(0, 3).map((d) => (
              <li key={d.path} className="truncate text-2xs text-muted-foreground" title={d.path}>
                {d.reason}
              </li>
            ))}
            {diagnostics.length > 3 && (
              <li className="text-2xs text-muted-foreground">…and {diagnostics.length - 3} more</li>
            )}
          </ul>
        </div>
      )}
    </div>
  )

  if (!isDrawerMode) {
    return (
      <div ref={containerRef} className="flex h-full min-w-0 border-r border-border">
        {body}
      </div>
    )
  }

  // Drawer mode: the column collapses to a thin strip with a reopen toggle
  // (the mockup just does `display:none` at <=620px with no way back --
  // that's the gap this task brief calls out). The toggle is always visible
  // so the affordance survives even when the drawer is closed.
  return (
    <div ref={containerRef} className="relative flex h-full min-w-0">
      <div className="flex-none border-r border-border bg-panel">
        <button
          type="button"
          onClick={() => setDrawerOpen((v) => !v)}
          aria-expanded={drawerOpen}
          aria-label={drawerOpen ? 'Close chat list' : 'Open chat list'}
          className="flex h-10 w-9 items-center justify-center text-muted-foreground hover:bg-panel2 hover:text-foreground"
        >
          {drawerOpen ? <PanelLeftClose size={15} /> : <PanelLeftOpen size={15} />}
        </button>
      </div>
      {drawerOpen && (
        <>
          {/* Click-outside backdrop; keyboard users close via the toggle or Escape. */}
          <button
            type="button"
            aria-label="Close chat list"
            onClick={() => setDrawerOpen(false)}
            className="fixed inset-0 z-40 cursor-default bg-black/40"
          />
          <div
            className={cn('absolute inset-y-0 left-9 z-50 shadow-lg')}
            style={{ width: SIDEBAR_MIN_PX }}
          >
            {body}
          </div>
        </>
      )}
    </div>
  )
}

const GROUPINGS: { id: SidebarGroupMode; label: string; icon: typeof Clock3 }[] = [
  { id: 'recent', label: 'Last updated', icon: Clock3 },
  { id: 'project', label: 'Project', icon: Folders },
  { id: 'diff', label: 'Changed files', icon: FileDiff },
]

/**
 * Every way to narrow or arrange the chat list, behind one button beside the
 * search box. These were three stacked rows (Active/Archived tabs, a project
 * checkbox, Recent/Project/Diff tabs) above a list most people never filter.
 * The button lights up while anything other than the defaults is on.
 */
function ChatListFilterMenu({
  grouping,
  onGroupingChange,
  showArchived,
  onShowArchivedChange,
  scopeToCurrentRepo,
  onScopeToCurrentRepoChange,
  currentRepoId,
  currentRepoName,
}: {
  grouping: SidebarGroupMode
  onGroupingChange: (mode: SidebarGroupMode) => void
  showArchived: boolean
  onShowArchivedChange: (archived: boolean) => void
  scopeToCurrentRepo: boolean
  onScopeToCurrentRepoChange: (scoped: boolean) => void
  currentRepoId: string | null
  currentRepoName: string | null
}) {
  const changed = showArchived || scopeToCurrentRepo || grouping !== 'recent'
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <button
          type="button"
          aria-label="Filter and arrange chats"
          title="Filter and arrange chats"
          className={cn(
            'flex h-[24px] w-[24px] flex-none items-center justify-center rounded border border-transparent text-muted-foreground',
            'hover:border-border hover:bg-panel2 hover:text-foreground',
            changed && 'border-primary/60 bg-soft text-accent-text'
          )}
        >
          <ListFilter size={12} aria-hidden />
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="w-52">
        <DropdownMenuSub>
          <DropdownMenuSubTrigger className="text-xs">
            Grouping
            <span className="ml-auto pl-3 text-2xs text-muted-foreground">
              {GROUPINGS.find((g) => g.id === grouping)?.label}
            </span>
          </DropdownMenuSubTrigger>
          <DropdownMenuSubContent>
            <DropdownMenuRadioGroup value={grouping} onValueChange={(v) => onGroupingChange(v as SidebarGroupMode)}>
              {GROUPINGS.map((g) => (
                <DropdownMenuRadioItem key={g.id} value={g.id} className="text-xs">
                  <g.icon size={12} aria-hidden className="mr-1.5 text-muted-foreground" />
                  {g.label}
                </DropdownMenuRadioItem>
              ))}
            </DropdownMenuRadioGroup>
          </DropdownMenuSubContent>
        </DropdownMenuSub>
        <DropdownMenuSub>
          <DropdownMenuSubTrigger className="text-xs">
            Show
            <span className="ml-auto pl-3 text-2xs text-muted-foreground">
              {showArchived ? 'Archived' : 'Active'}
            </span>
          </DropdownMenuSubTrigger>
          <DropdownMenuSubContent className="w-56">
            <DropdownMenuCheckboxItem
              className="text-xs"
              checked={scopeToCurrentRepo}
              disabled={!currentRepoId}
              onCheckedChange={(checked) => onScopeToCurrentRepoChange(checked === true)}
            >
              {currentRepoId ? `Only ${currentRepoName ?? 'this project'}` : 'Only this project (still opening)'}
            </DropdownMenuCheckboxItem>
            <DropdownMenuCheckboxItem
              className="text-xs"
              checked={showArchived}
              onCheckedChange={(checked) => onShowArchivedChange(checked === true)}
            >
              Archived chats
            </DropdownMenuCheckboxItem>
          </DropdownMenuSubContent>
        </DropdownMenuSub>
        {changed && (
          <>
            <DropdownMenuSeparator />
            <DropdownMenuItem
              className="text-xs"
              onSelect={() => {
                onGroupingChange('recent')
                onShowArchivedChange(false)
                onScopeToCurrentRepoChange(false)
              }}
            >
              Reset filters
            </DropdownMenuItem>
          </>
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
