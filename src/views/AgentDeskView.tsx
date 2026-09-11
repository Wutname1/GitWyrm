import { useEffect, useMemo, useRef, useState } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import { listen } from '@tauri-apps/api/event'
import { toast } from 'sonner'
import {
  commands,
  type CreateSessionRequest,
  type RepoInfo,
  type SelectDeskTarget,
  type SelectSessionTarget,
} from '@/lib/bindings'
import { unwrap, keys } from '@/lib/queryKeys'
import { describeError, log } from '@/lib/log'
import { describeOutcome } from '@/lib/agentDeskResult'
import { readWindowMode, type WindowMode } from '@/lib/windowMode'
import { AgentDeskTitleBar } from '@/components/domain/agent-desk/AgentDeskTitleBar'
import { AgentWorkspaceToolbar } from '@/components/domain/agent-desk/AgentWorkspaceToolbar'
import { SessionSidebar } from '@/components/domain/agent-desk/SessionSidebar'
import { AgentSetupView } from '@/components/domain/agent-setup/AgentSetupView'
import { ImportPicker } from '@/components/domain/agent-desk/ImportPicker'
import { ConversationPane } from '@/components/domain/agent-desk/ConversationPane'
import { PaneDetailPopover } from '@/components/domain/agent-desk/PaneDetailPopover'
import { DockedDetailPanel } from '@/components/domain/agent-desk/DockedDetailPanel'
import { AgentDeskDockDropZone } from '@/components/domain/agent-desk/AgentDeskDockDropZones'
import { OpenSpecEmbeddedDetail } from '@/components/domain/agent-desk/OpenSpecEmbeddedDetail'
import { useAgentSession, useAgentSessionExistence, useAgentSessionHeaders } from '@/hooks/useAgentSessions'
import { useOrphanResultReconciliation } from '@/hooks/useOrphanResultReconciliation'
import { useContainerWidth } from '@/hooks/useContainerWidth'
import { dockKindLabel, resolveDrop, resolvePin, resolveResponsiveMode, resolveSplitPresentation, shouldHideButtonLabels, zoneLabel } from '@/lib/agentDeskDock'
import { useAgentDeskUiStore } from '@/stores/agentDeskUiStore'
import { cn } from '@/lib/utils'
import { resolveAgentDeskShellState } from '@/views/agentDeskViewState'
import { otherPane, resolvePaneTarget, type PaneId } from '@/lib/agentDeskPaneTargeting'
import { isRightDockSafeAtWidth, resolveDockVisibility, zoneToPlacement, placementToZone, type DockZone } from '@/lib/agentDeskDockPlacement'
import type { DockKind } from '@/lib/agentWorkspaceLayout'
import { ConfirmDialog } from '@/components/modals/ConfirmDialog'

/** Matches the Rust side's `agent-desk://select-target` in `spec_desk.rs`. */
const SELECT_DESK_TARGET_EVENT = 'agent-desk://select-target'

/**
 * Matches the Rust side's `agent-desk://select-session` in
 * `agent_kickoff.rs` (`SELECT_SESSION_EVENT`) -- R3.3: a targeted
 * select-session event fired right after `agent_session_start` resolves, so
 * a Fix click lands on its own session instead of relying on
 * `agentSessionsAll` invalidating and `AgentDeskView`'s "land on the most
 * recent session" effect happening to guess right (that effect only runs
 * when no pane has a session yet -- it does nothing once any chat has ever
 * been opened, which is the common case a kickoff fires into).
 */
const SELECT_SESSION_EVENT = 'agent-desk://select-session'

/**
 * The window's current target (repo, and optionally a change), kept live
 * instead of frozen at first paint.
 *
 * `open_spec_desk` (backend) focuses this window and retargets it whenever
 * "Open Agent Desk" fires for a *different* repository -- the window is
 * app-wide now, so a second kickoff does not create a second window, it
 * redirects this one. The URL query params (`?repo=`/`path=`/`change=`) are
 * only ever right for the window's first paint; every retarget after that
 * arrives solely as `SELECT_DESK_TARGET_EVENT`. Without a live listener,
 * this window shows the first repo it was ever opened for, forever -- which
 * is Finding 3.
 *
 * First paint still reads the URL directly rather than waiting on the event:
 * `WebviewWindowBuilder::build()` (Rust) returns once the webview exists, not
 * once its page has subscribed, so an event fired immediately after creating
 * a fresh window can be missed entirely. Falling back to "whatever the event
 * says, once it shows up" would leave a freshly created window blank in that
 * race. Seeding from the URL sidesteps it: the URL is written into the
 * window's own creation call, so it is never subject to a subscription race,
 * and every kickoff (including the one that just created this window) also
 * fires the event, so a window that outlives its first paint stays correct
 * too.
 */
function useDeskTarget(): WindowMode {
  const [target, setTarget] = useState<WindowMode>(readWindowMode)

  useEffect(() => {
    const unlisten = listen<SelectDeskTarget>(SELECT_DESK_TARGET_EVENT, (event) => {
      const { repoId, repoPath, changeId } = event.payload
      setTarget({ kind: 'agent-desk', repoId, repoPath, changeId })
    })
    return () => {
      void unlisten.then((fn) => fn())
    }
  }, [])

  return target
}

/**
 * Opens the Desk's repository in this window's backend session.
 *
 * Mirrors `useDeskRepo` in `SpecDeskView.tsx`: this is a separate webview
 * with its own empty store, so it opens the repo itself. `open_repo` reuses
 * the handle when the path is already open.
 */
function useDeskRepo(repoPath: string | null) {
  const [repo, setRepo] = useState<RepoInfo | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    if (!repoPath) {
      setError('This window was opened without a repository.')
      return
    }
    let cancelled = false
    setError(null)
    setRepo(null)
    void (async () => {
      try {
        const info = unwrap(await commands.openRepo(repoPath))
        if (!cancelled) setRepo(info)
      } catch (e) {
        const message = describeError(e)
        log.error(`agent desk: could not open its repository: ${message}`)
        if (!cancelled) setError(message)
      }
    })()
    return () => {
      cancelled = true
    }
  }, [repoPath])

  return { repo, error }
}

/**
 * Calls `commands.agentSessionOpenSource` (`commands::agent_desk::agent_session_open_source`)
 * for one session, and surfaces the honest outcome as a toast -- Rule #1:
 * every action gives visible feedback, including "the main window is not
 * open yet" (a very early startup race) or a load failure for the session
 * itself, neither of which the button click alone would explain.
 *
 * This is the piece that was missing: `SessionSourceBanner`'s "View source"
 * button, and every `source`-kind message target link, have accepted an
 * `onOpenSource` prop since they shipped, but no caller in this file ever
 * passed one, so the button always rendered disabled (`disabled={!onOpenSource}`
 * in `SessionSourceBanner.tsx`/`SessionSourcePanel.tsx`/`EventStack.tsx`).
 */
function openSourceFor(sessionId: string) {
  void (async () => {
    try {
      const result = await commands.agentSessionOpenSource(sessionId)
      if (result.status === 'error') {
        toast.error(`Could not open the source: ${result.error}`)
        return
      }
      const outcome = unwrap(result)
      switch (outcome.kind) {
        case 'opened':
          break
        case 'mainWindowNotOpen':
          toast.error('The main GitWyrm window is not open yet.')
          break
        case 'sessionNotFound':
          toast.error('This chat could not be found.')
          break
        case 'sessionDamaged':
          toast.error(`This chat's data looks damaged: ${outcome.reason}`)
          break
        case 'sessionUnavailable':
          toast.error(`Could not read this chat right now: ${outcome.detail}`)
          break
      }
    } catch (e) {
      log.error(`agent desk: could not open source: ${describeError(e)}`)
      toast.error('Could not open the source.')
    }
  })()
}

function CenteredMessage({ title, detail }: { title: string; detail: string }) {
  return (
    <div className="flex flex-1 items-center justify-center p-8">
      <div className="max-w-sm text-center">
        <p className="text-sm font-semibold text-foreground">{title}</p>
        <p className="mt-1.5 text-xs leading-relaxed text-muted-foreground">{detail}</p>
      </div>
    </div>
  )
}

/**
 * One conversation pane plus its own Source/Context/Graph header buttons and
 * popover -- kept as a small local component (not a new file) since
 * everything it needs (`headerAnchorRef`, the popover) is scoped to exactly
 * one pane instance and never shared with its sibling.
 */
function AgentDeskPane({
  pane,
  sessionId,
  isActive,
  onFocusPane,
  onPin,
  detailSession,
  showSourceBanner,
}: {
  pane: PaneId
  sessionId: string | null
  isActive: boolean
  onFocusPane: (pane: PaneId) => void
  onPin: (pane: PaneId, kind: DockKind, edge: 'left' | 'right' | 'bottom') => void
  detailSession: ReturnType<typeof useAgentSession>['session']
  /**
   * tasks.md 8.1/8.2: the workspace toolbar's visibility toggle reaches the
   * big source bar above each transcript -- and only that. The Source button
   * in `headerSlot` below is deliberately not gated on it, so hiding the bars
   * never takes the information away, it just stops it taking up room.
   */
  showSourceBanner: boolean
}) {
  const headerAnchorRef = useRef<HTMLDivElement | null>(null)
  return (
    <section
      data-pane={pane}
      onFocusCapture={() => onFocusPane(pane)}
      onMouseDownCapture={() => onFocusPane(pane)}
      className={cn(
        'flex min-h-0 flex-1 flex-col outline-none',
        // tasks.md 4.2/5.3: an unmistakable, inset accent border marks the
        // active pane, matching the mockup's `.ag-pane.is-active` treatment.
        isActive && 'shadow-[inset_0_2px_0_var(--gw-accent)]'
      )}
      aria-label={isActive ? 'Active conversation pane' : 'Conversation pane'}
    >
      <ConversationPane
        sessionId={sessionId}
        isActive={isActive}
        paneLabel={pane === 'primary' ? 'First chat' : 'Second chat'}
        showSourceBanner={showSourceBanner}
        headerAnchorRef={headerAnchorRef}
        onOpenSource={sessionId ? () => openSourceFor(sessionId) : undefined}
        headerSlot={
          <PaneDetailPopover
            session={detailSession}
            headerAnchorRef={headerAnchorRef}
            onOpenSource={sessionId ? () => openSourceFor(sessionId) : undefined}
            onPin={(kind, edge) => onPin(pane, kind, edge)}
          />
        }
      />
    </section>
  )
}

/**
 * The Agent Desk window: dense session navigation on the left, one or two
 * active conversation panes in the center (Split View), and a shared
 * Context/Graph/Source dock that follows the active pane -- plus OpenSpec
 * detail as a full-width alternate center view.
 *
 * Pane/dock/draft state now lives in `useAgentDeskUiStore`
 * (`src/stores/agentDeskUiStore.ts`) rather than local `useState`: that
 * store is the persisted, restart-surviving source of truth for the layout
 * (architecture.md section 5), and this view is only ever a reader/writer of
 * it through its documented API (`setPaneSession`, `setActivePane`,
 * `openSplit`/`closeSplit`, `openDock`/`moveDock`/`resizeDock`/`closeDock`,
 * `resetLayout`, `setSourceBarsVisible`/`toggleSourceBarsVisible`).
 */
export function AgentDeskView() {
  const mode = useDeskTarget()
  const { repo, error } = useDeskRepo(mode.repoPath)
  const repoId = repo?.id ?? null
  const qc = useQueryClient()

  const hydrated = useAgentDeskUiStore((s) => s.hydrated)
  const hydrate = useAgentDeskUiStore((s) => s.hydrate)
  const layout = useAgentDeskUiStore((s) => s.layout)
  const setPaneSession = useAgentDeskUiStore((s) => s.setPaneSession)
  const setActivePane = useAgentDeskUiStore((s) => s.setActivePane)
  const restorePaneToFallback = useAgentDeskUiStore((s) => s.restorePaneToFallback)
  const openSplit = useAgentDeskUiStore((s) => s.openSplit)
  const closeSplit = useAgentDeskUiStore((s) => s.closeSplit)
  const setSourceBarsVisible = useAgentDeskUiStore((s) => s.setSourceBarsVisible)
  const toggleSourceBarsVisible = useAgentDeskUiStore((s) => s.toggleSourceBarsVisible)
  const openDock = useAgentDeskUiStore((s) => s.openDock)
  const moveDock = useAgentDeskUiStore((s) => s.moveDock)
  const resizeDock = useAgentDeskUiStore((s) => s.resizeDock)
  const closeDock = useAgentDeskUiStore((s) => s.closeDock)
  const resetLayout = useAgentDeskUiStore((s) => s.resetLayout)

  // tasks.md 8.5: restore layout before first paint, not after -- runs once,
  // synchronously in an effect with no dependency on anything that could
  // delay it (repo open, session list) so the pane grid never flashes from
  // "single pane" to "restored split" a frame after mount.
  useEffect(() => {
    hydrate(window.innerWidth)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  // In the store (not local state) so a message link inside a pane can open
  // the Spec view -- see `agentDeskUiStore.centerView`.
  const centerView = useAgentDeskUiStore((s) => s.centerView)
  const setCenterView = useAgentDeskUiStore((s) => s.setCenterView)
  const [creating, setCreating] = useState(false)
  const [resetConfirmOpen, setResetConfirmOpen] = useState(false)
  const [windowWidth, setWindowWidth] = useState(() => window.innerWidth)
  const composerFocusRef = useRef<HTMLDivElement | null>(null)

  useEffect(() => {
    const onResize = () => setWindowWidth(window.innerWidth)
    window.addEventListener('resize', onResize)
    return () => window.removeEventListener('resize', onResize)
  }, [])

  // Re-fit the pinned panel when the window shrinks.
  //
  // `clampDockSizePx` takes the tighter of the 70% rule and
  // `windowWidth - MIN_CHAT_SIZE_PX`, so a dock can never squeeze the
  // conversation below its documented minimum -- but it only ran when the
  // layout was restored or the divider was dragged. Nothing re-applied it when
  // the WINDOW changed, so sizing a dock wide while maximised and then
  // restoring the window left the conversation under that minimum, with no
  // sign that dragging the divider once would fix it.
  //
  // `resizeDock` clamps internally, so passing the current size back through it
  // is a no-op whenever the size is already legal.
  const dockSizePx = layout.dock?.sizePx
  useEffect(() => {
    if (dockSizePx == null) return
    resizeDock(dockSizePx, windowWidth)
  }, [windowWidth, dockSizePx, resizeDock])

  /**
   * tasks.md 5.8: the conversation column's own width, not the window's,
   * decides whether the panes sit side by side, stack, or reduce to one pane
   * plus a switcher.
   *
   * Measured rather than taken from a viewport media query because this
   * column shares the window with the chat list and (often) a docked panel:
   * a wide window with a wide left dock still leaves the panes cramped, and
   * a `min-[761px]` utility class cannot see that. `SessionSidebar` already
   * measures its own container the same way, for the same reason.
   */
  const paneAreaRef = useRef<HTMLDivElement | null>(null)
  const paneAreaWidth = useContainerWidth(paneAreaRef)
  const responsiveMode = resolveResponsiveMode(paneAreaWidth)
  const splitPresentation = resolveSplitPresentation(layout.split, responsiveMode)

  const primarySessionId = layout.primarySessionId
  const secondarySessionId = layout.secondarySessionId
  const activePane = layout.activePane

  // The dock (and each pane's popover) needs the whole session, not just its
  // id. Both read through the same cached `useAgentSession` query
  // `ConversationPane` uses, so following the active pane never refetches.
  const { session: primaryDetailSession } = useAgentSession(primarySessionId)
  const { session: secondaryDetailSession } = useAgentSession(secondarySessionId)
  const activeDetailSession = activePane === 'secondary' ? secondaryDetailSession : primaryDetailSession

  // R4.2: retargeting the main window's repo (a different kickoff fired
  // "Open Agent Desk" for a different project) is announced, but it no
  // longer clears either pane's selection. Panes resolve their own repo
  // context per session (`useAgentSession` -> the session's own header),
  // never from this window's target, so there is nothing here that would go
  // stale -- a chat from the *previous* repo is exactly as valid to keep
  // looking at as one from the new repo, since Agent Desk is one app-wide
  // workspace, not a window bound to a single repository (R4.1/R4.3). The
  // OpenSpec and Setup center views *are* bound to the window's current repo
  // (they are not per-session panes), so a retarget does still leave those
  // and return to the conversation view, which stays valid across the switch.
  const previousRepoId = useRef(mode.repoId)
  const announceNextRepo = useRef(false)
  useEffect(() => {
    if (previousRepoId.current !== mode.repoId) {
      previousRepoId.current = mode.repoId
      announceNextRepo.current = true
      setCenterView('conversation')
    }
  }, [mode.repoId])

  useEffect(() => {
    if (repo && announceNextRepo.current) {
      announceNextRepo.current = false
      toast.info(`Switched to ${repo.name}`)
    }
  }, [repo])

  // App-wide by default (R4.1): this powers the "empty workspace"/"land on
  // most recent chat" logic below, so both must see every project's
  // sessions, not just the main window's current one -- otherwise opening
  // Agent Desk for a *second* repo that has no chats of its own would show
  // the empty state even though other projects have plenty, and "most
  // recent" would only ever mean "most recent in this one repo". The
  // sidebar applies its own, separately-scoped "This project only" filter
  // on top of this same app-wide list (`SessionSidebar`).
  const filter = {
    repoId: null,
    projectPath: null,
    states: [],
    sourceKinds: [],
    hasChangedFiles: null,
    archived: false,
    titleContains: null,
  }
  const { headers, isLoading: sessionsLoading, isError: sessionsErrored } =
    useAgentSessionHeaders(filter)

  // Does each pane's session still exist? Asked of the backend, per session.
  //
  // Two earlier fixes went through the session LIST for this: first widening
  // it to include archived chats, because an archived one read as deleted.
  // That is the wrong question. The list is filtered and paged, so membership
  // of it answers "is this chat in the first hundred unarchived ones", which
  // is not what the pane needs to know. `agentSessionGet` answers the actual
  // question and needs no filter to be kept in sync with it.
  const primaryOutcome = useAgentSessionExistence(primarySessionId)
  const secondaryOutcome = useAgentSessionExistence(layout.split ? secondarySessionId : null)

  // Land on the most recent session automatically so the window is never
  // just an empty pane the first time it opens with sessions already saved.
  useEffect(() => {
    if (hydrated && primarySessionId == null && secondarySessionId == null && headers.length > 0) {
      setPaneSession('primary', headers[0].sessionId)
    }
  }, [hydrated, headers, primarySessionId, secondarySessionId, setPaneSession])

  // tasks.md 4.5: a pane whose session no longer resolves (deleted/archived
  // out from under it) falls back to the newest valid session rather than
  // showing a dead reference forever.
  useEffect(() => {
    if (!hydrated || sessionsLoading) return
    // Ask about THIS session rather than looking for it in the list.
    //
    // The list is paged at 100 and nothing here asks for a second page, so a
    // pane restored onto an older chat was not in `headers` and read as
    // deleted -- silently swapped for the newest chat, which to the person
    // looks like their conversation was thrown away. Membership of a paged
    // list is not the same question as existence, and only the second one
    // matters here.
    //
    // `notFound` is the authoritative answer; `damaged` and `unavailable`
    // deliberately do NOT evict, because a file that cannot be read right now
    // is not a file that is gone, and the pane's own surfaces already say so.
    // `headers[0]` when there is one, otherwise null. This used to return
    // early whenever the list was empty, which is exactly the case of deleting
    // your LAST chat: the pane kept pointing at the session that had just been
    // removed, with nothing left to fall back to. `restorePaneToFallback`
    // already accepts null, so the empty case only needed to be allowed to
    // reach it. Gated on `sessionsLoading` so a list that has not arrived yet
    // is never mistaken for a list with nothing in it.
    const fallback = headers.length > 0 ? headers[0].sessionId : null
    if (primaryOutcome?.kind === 'notFound' && primarySessionId) {
      restorePaneToFallback('primary', fallback)
    }
    if (layout.split && secondaryOutcome?.kind === 'notFound' && secondarySessionId) {
      restorePaneToFallback('secondary', fallback)
    }
    // The two outcomes are dependencies: they are what the effect now branches
    // on, and leaving them out would run the check against whichever answer
    // was current when the effect last ran.
  }, [
    hydrated,
    sessionsLoading,
    headers,
    primaryOutcome,
    secondaryOutcome,
    primarySessionId,
    secondarySessionId,
    layout.split,
    restorePaneToFallback,
  ])

  /**
   * Put the caret in the composer's textarea, not the box around it.
   *
   * The pane re-renders when its session changes, so the textarea for a
   * brand-new chat does not exist yet at the moment "New chat" resolves --
   * hence the rAF, which lets that render land first. Falls back to the pane
   * wrapper if the textarea is somehow absent, so focus still moves somewhere
   * sensible rather than nowhere.
   */
  const focusComposer = (sessionId: string | null) => {
    requestAnimationFrame(() => {
      // By session, not by document order. A plain
      // `querySelector('[data-agent-desk-composer]')` always returns the
      // FIRST composer, so in Split View selecting a chat in the second pane
      // highlighted that pane while the caret landed in the other one.
      // Every composer carries its own session id, so the right one can be
      // named.
      const box = sessionId
        ? document.getElementById(`agent-desk-composer-${sessionId}`)
        : null
      if (box instanceof HTMLTextAreaElement) box.focus()
      else composerFocusRef.current?.focus()
    })
  }

  const onSelectSession = (sessionId: string) => {
    const target = resolvePaneTarget({
      split: layout.split,
      activePane: layout.activePane,
      primarySessionId,
      secondarySessionId,
      sessionId,
    })
    if (target.action === 'open') {
      setPaneSession(target.pane, sessionId)
    }
    setActivePane(target.pane)
    // Come back to the conversation if a full-screen section is covering it.
    // The sidebar stays mounted beside Agent Setup and the OpenSpec section,
    // so clicking a chat set the pane behind a screen the person could still
    // not see -- and the comment below promising "a click always produces a
    // visible response" was, in that case, describing something that did not
    // happen. The import section already did this; the other two were missed.
    setCenterView('conversation')
    // Rule #1: a click always produces a visible response. Selecting (or
    // refocusing) a session both highlights its pane (via `activePane`) and
    // moves focus into the composer so a keyboard user lands somewhere useful,
    // not on a stale focus target.
    focusComposer(sessionId)
  }

  // P1-C wiring 3: scan every session, once, for a Kept/CleanupNeeded result
  // whose worktree folder has vanished -- see the hook's own doc comment.
  // Reuses `onSelectSession` (not a bespoke navigation path) so the toast's
  // "open it" action behaves exactly like clicking that session in the
  // sidebar would.
  useOrphanResultReconciliation(onSelectSession)

  // R3.1/R3.3: a source kickoff (issue Fix, etc.) tells this window exactly
  // which session to show the instant it exists, rather than this window
  // having to guess from a query refetch. Routed through the same
  // `onSelectSession` a sidebar click uses, so a kickoff and a manual click
  // behave identically once the session id is known -- same pane-targeting,
  // same focus-the-composer feedback.
  const onSelectSessionRef = useRef(onSelectSession)
  onSelectSessionRef.current = onSelectSession
  useEffect(() => {
    const unlisten = listen<SelectSessionTarget>(SELECT_SESSION_EVENT, (event) => {
      onSelectSessionRef.current(event.payload.sessionId)
    })
    return () => {
      void unlisten.then((fn) => fn())
    }
  }, [])

  // tasks.md 2.4 ("rebuild [the OpenSpec context] on file-watcher refresh"):
  // the backend's watcher (`src-tauri/src/watcher.rs`) already emits
  // `repo-changed` whenever a tracked repository's files change (editor
  // save, terminal git command, an agent's own file writes) -- `App.tsx`
  // already listens for it via `useRepoWatcher`, but that hook is only ever
  // mounted in the MAIN window (grepped `src/views/*`: zero other mounters),
  // so an Agent Desk window never reacted to it at all before this. Each
  // pane's session carries its own `repoId` (R4.4: "resolve repo context per
  // session"), so this compares the event against each pane's session
  // rather than the window's own `mode.repoId` -- a pane can be showing a
  // chat from a different repo than the one this window was opened for.
  const primaryRepoId = primaryDetailSession?.header.repoId ?? null
  const secondaryRepoId = secondaryDetailSession?.header.repoId ?? null
  useEffect(() => {
    const unlisten = listen<{ repo_id: string }>('repo-changed', (event) => {
      const changedRepoId = event.payload.repo_id
      if (primarySessionId && primaryRepoId === changedRepoId) {
        void qc.invalidateQueries({ queryKey: keys.agentSessionOpenspecContext(primarySessionId) })
        void qc.invalidateQueries({ queryKey: keys.agentSessionOpenspecContextDrift(primarySessionId) })
        void qc.invalidateQueries({ queryKey: keys.agentSessionOpenspecStatus(primarySessionId) })
      }
      if (secondarySessionId && secondaryRepoId === changedRepoId) {
        void qc.invalidateQueries({ queryKey: keys.agentSessionOpenspecContext(secondarySessionId) })
        void qc.invalidateQueries({ queryKey: keys.agentSessionOpenspecContextDrift(secondarySessionId) })
        void qc.invalidateQueries({ queryKey: keys.agentSessionOpenspecStatus(secondarySessionId) })
      }
    })
    return () => {
      void unlisten.then((fn) => fn())
    }
  }, [qc, primarySessionId, secondarySessionId, primaryRepoId, secondaryRepoId])

  const onNewChat = async () => {
    if (!repo || creating) return
    setCreating(true)
    try {
      const request: CreateSessionRequest = {
        repoId: repo.id,
        repoPath: repo.path,
        repoName: repo.name,
        title: '',
        source: { kind: 'manual', repoId: repo.id },
        // `fix`, not `ask`. The intent is the ceiling on what a chat may ever
        // do -- `policy.rs` gives `ask` `canWrite: false` -- and a blank chat
        // has no purpose yet, so capping it at read-only decided for the
        // person before they had typed anything.
        //
        // What that looked like: Plan and Auto were both disabled on the new
        // chat screen, each showing Ask's own description ("This chat only
        // reads and explains, so it cannot change files") as the reason.
        // Meanwhile the composer defaulted its mode pill to Auto, so the
        // screen offered Auto and refused it at the same time.
        //
        // `fix` is the only intent that permits writes; Ask and Plan remain
        // one click away as MODES, which is the distinction the product
        // draws -- the intent says what the chat is for, the mode says how
        // much authority this turn gets. A chat started from an issue or a
        // review still gets its own narrower intent from the kickoff path,
        // which is untouched.
        intent: 'fix',
      }
      const outcome = unwrap(await commands.agentSessionCreate(request))
      if (outcome.kind === 'created') {
        // tasks.md 4.6: New chat replaces the *active* pane and focuses its composer.
        setPaneSession(layout.activePane, outcome.session.header.sessionId)
        void qc.invalidateQueries({ queryKey: keys.agentSessionsAll })
        // New chat is in the title bar, so it is reachable from every section.
        // Without this it reported "New chat started" while the chat it made
        // sat behind Agent Setup or the OpenSpec section -- a success message
        // for something the person could not see.
        setCenterView('conversation')
        toast.success('New chat started.')
        focusComposer(outcome.session.header.sessionId)
      } else {
        toast.error('Could not start a new chat.', { description: describeOutcome(outcome) })
      }
    } catch (e) {
      const message = describeError(e)
      log.error(`agent desk: could not create session: ${message}`)
      toast.error('Could not start a new chat.', { description: message })
    } finally {
      setCreating(false)
    }
  }

  const onFocusPane = (pane: PaneId) => {
    if (pane !== layout.activePane) setActivePane(pane)
  }

  const onPin = (pane: PaneId, kind: DockKind, edge: 'left' | 'right' | 'bottom') => {
    if (pane !== layout.activePane) setActivePane(pane)
    // The same width decision the toolbar's pin makes. This path used to skip
    // it and toast "pinned" unconditionally, so below the safe width it
    // reported success for a panel that fell back to a popover and never
    // appeared -- an action with no visible result.
    const outcome = resolvePin({
      targetZone: placementToZone(edge),
      rightZoneUnavailable: !isRightDockSafeAtWidth(windowWidth),
    })
    if (outcome.status === 'reject') {
      toast.info(outcome.reason)
      return
    }
    const replaced = layout.dock && layout.dock.kind !== kind ? layout.dock.kind : null
    openDock(kind, edge)
    // Only one panel can be pinned, so pinning a second one removes the first.
    // This path used to say nothing at all: the panel a person was watching
    // simply vanished, with no way to tell what had happened to it.
    toast.success(
      replaced
        ? `${dockKindLabel(kind)} pinned. ${dockKindLabel(replaced)} was unpinned to make room.`
        : `${dockKindLabel(kind)} pinned.`
    )
  }

  /**
   * The single move path behind all three ways to place a panel (tasks.md
   * 7.4/7.5/7.7): the Move menu, a pointer drop, and the keyboard all end up
   * here, so no destination is reachable by one route and not the others.
   *
   * `resolveDrop` decides whether the move is real, and a rejection always
   * carries a plain-language reason (7.6) -- so a drop that cannot be
   * honoured says why and leaves the panel exactly where it was, rather than
   * silently doing nothing and looking broken.
   */
  /**
   * Pin a panel to a chosen zone, through the same rejection path moving
   * already uses.
   *
   * Pinning used to hardcode the right edge, which is unavailable below a
   * window width -- so on a narrow window the only pinning affordance in the
   * product silently did nothing. Now every zone is offered and an unsafe one
   * says why instead of failing quietly.
   */
  const onPinDock = (kind: DockKind, zone: DockZone) => {
    const outcome = resolveDrop({
      targetZone: zone,
      dock: layout.dock,
      rightZoneUnavailable: !isRightDockSafeAtWidth(windowWidth),
    })
    if (outcome.status === 'reject') {
      toast.info(outcome.reason)
      return
    }
    const placement = zoneToPlacement(outcome.zone)
    const replaced = layout.dock && layout.dock.kind !== kind ? layout.dock.kind : null
    openDock(kind, placement.edge, placement.leftOrder)
    // Name what was displaced. "Panel pinned" alone left the person to notice
    // for themselves that a different panel had gone.
    toast.success(
      replaced
        ? `${dockKindLabel(kind)} pinned ${zoneLabel(outcome.zone).toLowerCase()}. ${dockKindLabel(replaced)} was unpinned to make room.`
        : `${dockKindLabel(kind)} pinned ${zoneLabel(outcome.zone).toLowerCase()}.`
    )
  }

  const onMoveDock = (zone: DockZone) => {
    const outcome = resolveDrop({
      targetZone: zone,
      dock: layout.dock,
      // The right edge is only offered while it is actually safe -- below
      // that width a right dock would crush the conversation (7.10).
      rightZoneUnavailable: !isRightDockSafeAtWidth(windowWidth),
    })
    if (outcome.status === 'reject') {
      toast.info(outcome.reason)
      return
    }
    const placement = zoneToPlacement(outcome.zone)
    moveDock(placement.edge, placement.leftOrder)
    toast.success(`Panel moved: ${zoneLabel(outcome.zone).toLowerCase()}.`)
  }

  const onDockDrop = onMoveDock

  // tasks.md 8.4: Ctrl+Alt+S toggles the source bars, and Ctrl+1/Ctrl+2
  // focus a pane. Scoped to this window only, following App.tsx's own
  // addEventListener idiom (there is no command-palette/keybinding registry
  // to hook into instead).
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.ctrlKey && e.altKey && (e.key === 's' || e.key === 'S')) {
        e.preventDefault()
        toggleSourceBarsVisible()
        return
      }
      if (e.ctrlKey && !e.altKey && !e.metaKey) {
        if (e.key === '1') {
          e.preventDefault()
          // Described as focusing a pane, and they used to only mark one
          // active: the dock and sidebar followed, but the caret stayed in the
          // composer just left, so typing went to the pane the person had
          // visibly moved away from.
          setActivePane('primary')
          focusComposer(primarySessionId)
        } else if (e.key === '2' && layout.split) {
          e.preventDefault()
          setActivePane('secondary')
          focusComposer(secondarySessionId)
        } else if (e.key === '\\') {
          // Focus-the-other-pane, a lightweight complement to Ctrl+1/2 when split.
          if (layout.split) {
            e.preventDefault()
            const next = otherPane(layout.activePane)
            setActivePane(next)
            focusComposer(next === 'secondary' ? secondarySessionId : primarySessionId)
          }
        }
      }
    }
    document.addEventListener('keydown', onKeyDown)
    return () => document.removeEventListener('keydown', onKeyDown)
    // The session ids are dependencies now that the handler focuses a
    // pane's composer by id: without them this listener would keep whichever
    // ids were current when it was attached and focus a stale chat.
  }, [layout.split, layout.activePane, toggleSourceBarsVisible, setActivePane, primarySessionId, secondarySessionId])

  const repoName = repo?.name ?? 'Loading…'

  const shellState = resolveAgentDeskShellState({
    hasRepoPath: mode.repoPath != null,
    repoError: mode.repoPath != null ? error : null,
    repoReady: repo != null,
    sessionsLoading,
    sessionCount: headers.length,
    hasSelection: primarySessionId != null || secondarySessionId != null,
  })

  const dockVisibility = useMemo(() => resolveDockVisibility(layout.dock, windowWidth), [layout.dock, windowWidth])
  // tasks.md 7.10: at unsafe widths a pinned right dock falls back to being
  // reachable only through each pane's own popover -- the dock's session
  // stays exactly what it was, just not rendered pinned, so re-widening the
  // window (or a resize back above the threshold) brings it back with no
  // extra state to reconcile.
  const showPinnedDock = layout.dock != null && dockVisibility === 'pinned'

  return (
    <div className="flex h-screen w-screen flex-col overflow-hidden bg-bg text-foreground">
      <AgentDeskTitleBar
        repoName={repoName}
        repoId={repoId}
        onNewChat={() => void onNewChat()}
        centerView={centerView}
        onChangeCenterView={setCenterView}
      />

      {/* Explicit no-repository state: the window was opened with nothing to
          act on at all -- distinct from a repo that failed to open, and from
          one that is merely slow to open. */}
      {shellState === 'no-repository' ? (
        <CenteredMessage
          title="Agent Desk has nothing to show"
          detail="This window was opened without a repository."
        />
      ) : shellState === 'load-failed' ? (
        /* Explicit load-failed state: the repo path was known, but opening
           it failed -- a different problem than never having one. */
        <CenteredMessage title="This repository could not be opened" detail={error ?? 'Unknown error.'} />
      ) : shellState === 'opening' || !repo ? (
        /* Explicit opening state, matching SpecDeskView's pattern: a named
           message, not a bare spinner. */
        <CenteredMessage
          title="Opening the repository…"
          detail="This takes a moment on first open."
        />
      ) : (
        <div className="agent-desk-columns relative flex min-h-0 flex-1">
          {sessionsErrored ? (
            /* Explicit load-failed state for the session list itself. */
            <div
              className="flex min-h-0 flex-none flex-col items-center justify-center gap-1 border-r border-border bg-panel p-4 text-center"
              style={{ width: 240 }}
            >
              <p className="text-xs font-semibold text-foreground">Chats could not load</p>
              <p className="max-w-[14rem] text-2xs leading-relaxed text-muted-foreground">
                Try again, or restart Agent Desk if this keeps happening.
              </p>
            </div>
          ) : (
            <SessionSidebar
              currentRepoId={repoId}
              currentRepoName={repo?.name ?? null}
              selectedId={activePane === 'secondary' ? secondarySessionId : primarySessionId}
              onSelectSession={onSelectSession}
              onNewSession={() => void onNewChat()}
            />
          )}

          {centerView === 'import' ? (
            /* Mounted only in this branch, never alongside the sidebar: the
               adapter scan starts when this component renders, so gating it
               here keeps chat loading from ever waiting on a filesystem sweep
               of other apps' session stores. */
            <ImportPicker
              onOpenSession={(sessionId) => {
                // Continuing an imported chat should land you in it. The
                // import view replaces the conversation, so without this the
                // person was told it was continuing and left looking at the
                // list they started from.
                onSelectSession(sessionId)
                setCenterView('conversation')
              }}
            />
          ) : centerView === 'setup' ? (
            /* Agent setup takes over the centre the way OpenSpec does: it is a
               whole workspace of its own, not a panel. Closing returns to the
               conversation rather than to wherever you were, because the setup
               view can be reached from either. */
            <AgentSetupView repoId={repo.id} onClose={() => setCenterView('conversation')} />
          ) : centerView === 'openspec' ? (
            /* Task 2.3: keep current OpenSpec details/actions functional.
               Full-width so DeskDetail/DeskActionRail/DeskChangesList render
               at the same proportions they always have, rather than being
               squeezed into the right dock meant for Context/Graph/Source. */
            <OpenSpecEmbeddedDetail repoId={repo.id} repoPath={repo.path} />
          ) : (
            <>
              <div ref={composerFocusRef} tabIndex={-1} className="relative flex min-h-0 flex-1 flex-col outline-none">
                {/* Workspace toolbar: Split View, source-bar visibility, the
                    panel/dock menu, and the reset-layout escape hatch
                    (tasks.md 5.1, 7.4, 8.1, 8.6). */}
                <AgentWorkspaceToolbar
                  split={layout.split}
                  onToggleSplit={() => (layout.split ? closeSplit() : openSplit())}
                  sourceBarsVisible={layout.sourceBarsVisible}
                  onToggleSourceBars={toggleSourceBarsVisible}
                  dock={layout.dock}
                  onMoveDock={onMoveDock}
                  onUnpinDock={closeDock}
                  onPinDock={onPinDock}
                  onResetLayout={() => setResetConfirmOpen(true)}
                  hideLabels={shouldHideButtonLabels(responsiveMode)}
                />

                {/* Explicit empty state when there are no chats at all yet:
                    SessionSidebar already covers "no rows"; here the
                    center column explains what a first click gets you. */}
                {shellState === 'empty' ? (
                  <div className="flex flex-1 flex-col items-center justify-center gap-2 p-8 text-center">
                    <p className="text-sm font-semibold text-foreground">Start your first chat</p>
                    <p className="max-w-sm text-xs leading-relaxed text-muted-foreground">
                      Use New chat above, or right-click an issue, pull request, or OpenSpec task
                      in the main window and choose an AI action.
                    </p>
                  </div>
                ) : (
                  <div
                    className={cn(
                      'flex min-h-0 flex-1',
                      layout.split ? 'flex-col min-[761px]:flex-row' : 'flex-row'
                    )}
                  >
                    {/* left-above/left-below docks sandwich the pane grid inside this row/column so both orders are simple flex placement, no absolute positioning. */}
                    {showPinnedDock && layout.dock!.edge === 'left' && layout.dock!.leftOrder !== 'below-chats' && (
                      <DockedDetailPanel
                        kind={layout.dock!.kind}
                        edge="left"
                        leftOrder={layout.dock!.leftOrder}
                        sizePx={layout.dock!.sizePx}
                        session={activeDetailSession}
                        onOpenSource={activeDetailSession ? () => openSourceFor(activeDetailSession.header.sessionId) : undefined}
                        onMove={onMoveDock}
                        onResize={(px) => resizeDock(px, windowWidth)}
                        onResizeReset={() => resizeDock(360, windowWidth)}
                        onUnpin={closeDock}
                      />
                    )}

                    <div className={cn('flex min-h-0 flex-1', layout.split ? 'flex-col' : 'flex-row')}>
                      {/* tasks.md 5.8, three presentations driven by the
                          measured column width (`splitPresentation`):
                          side-by-side when wide, stacked at the compact
                          breakpoint so neither composer is clipped, and one
                          pane plus a switcher when narrow. The switcher is
                          what keeps the hidden pane reachable -- the mockup
                          simply drops content at its narrow breakpoint with
                          no way back, which house Rule #1 does not allow. */}
                      <div ref={paneAreaRef} className="flex min-h-0 flex-1 flex-col">
                        {layout.split && splitPresentation === 'active-only' && (
                          <div
                            role="tablist"
                            aria-label="Which chat to show"
                            className="flex flex-none gap-1 border-b border-border bg-panel px-2 py-1"
                          >
                            {(['primary', 'secondary'] as const).map((pane) => (
                              <button
                                key={pane}
                                role="tab"
                                aria-selected={activePane === pane}
                                onClick={() => onFocusPane(pane)}
                                className={cn(
                                  'rounded px-2 py-1 text-2xs font-semibold',
                                  activePane === pane
                                    ? 'bg-soft text-accent-text'
                                    : 'text-sub hover:bg-panel3 hover:text-foreground'
                                )}
                              >
                                {pane === 'primary' ? 'First chat' : 'Second chat'}
                              </button>
                            ))}
                          </div>
                        )}

                        <div
                          className={cn(
                            'flex min-h-0 flex-1',
                            splitPresentation === 'stacked' ? 'flex-col' : 'flex-row'
                          )}
                        >
                          {splitPresentation === 'active-only' ? (
                            /* One pane VISIBLE. When the split is open, both
                               panes stay mounted and the inactive one is
                               hidden -- swapping which element exists would
                               unmount the pane being left, and an unmounted
                               transcript loses its scroll position, so
                               Ctrl+1/Ctrl+2 kept snapping the reader back to
                               the newest message. When the split is closed
                               there is only ever the primary pane. */
                            <>
                              <div
                                className={cn('flex min-h-0 flex-1', layout.split && activePane === 'secondary' && 'hidden')}
                              >
                                <AgentDeskPane
                                  pane="primary"
                                  sessionId={primarySessionId}
                                  isActive={!layout.split || activePane === 'primary'}
                                  onFocusPane={onFocusPane}
                                  onPin={onPin}
                                  showSourceBanner={layout.sourceBarsVisible}
                                  detailSession={primaryDetailSession}
                                />
                              </div>
                              {layout.split && (
                                <div className={cn('flex min-h-0 flex-1', activePane !== 'secondary' && 'hidden')}>
                                  <AgentDeskPane
                                    pane="secondary"
                                    sessionId={secondarySessionId}
                                    isActive
                                    onFocusPane={onFocusPane}
                                    onPin={onPin}
                                    showSourceBanner={layout.sourceBarsVisible}
                                    detailSession={secondaryDetailSession}
                                  />
                                </div>
                              )}
                            </>
                          ) : (
                            <>
                              <AgentDeskPane
                                pane="primary"
                                sessionId={primarySessionId}
                                isActive={activePane === 'primary'}
                                onFocusPane={onFocusPane}
                                onPin={onPin}
                                showSourceBanner={layout.sourceBarsVisible}
                                detailSession={primaryDetailSession}
                              />
                              <AgentDeskPane
                                pane="secondary"
                                sessionId={secondarySessionId}
                                isActive={activePane === 'secondary'}
                                onFocusPane={onFocusPane}
                                onPin={onPin}
                                showSourceBanner={layout.sourceBarsVisible}
                                detailSession={secondaryDetailSession}
                              />
                            </>
                          )}
                        </div>
                      </div>

                      {showPinnedDock && layout.dock!.edge === 'bottom' && (
                        <DockedDetailPanel
                          kind={layout.dock!.kind}
                          edge="bottom"
                          sizePx={layout.dock!.sizePx}
                          session={activeDetailSession}
                          onOpenSource={activeDetailSession ? () => openSourceFor(activeDetailSession.header.sessionId) : undefined}
                          onMove={onMoveDock}
                          onResize={(px) => resizeDock(px, windowWidth)}
                          onResizeReset={() => resizeDock(360, windowWidth)}
                          onUnpin={closeDock}
                        />
                      )}
                    </div>

                    {showPinnedDock && layout.dock!.edge === 'left' && layout.dock!.leftOrder === 'below-chats' && (
                      <DockedDetailPanel
                        kind={layout.dock!.kind}
                        edge="left"
                        leftOrder={layout.dock!.leftOrder}
                        sizePx={layout.dock!.sizePx}
                        session={activeDetailSession}
                        onOpenSource={activeDetailSession ? () => openSourceFor(activeDetailSession.header.sessionId) : undefined}
                        onMove={onMoveDock}
                        onResize={(px) => resizeDock(px, windowWidth)}
                        onResizeReset={() => resizeDock(360, windowWidth)}
                        onUnpin={closeDock}
                      />
                    )}

                    {showPinnedDock && layout.dock!.edge === 'right' && (
                      <DockedDetailPanel
                        kind={layout.dock!.kind}
                        edge="right"
                        sizePx={layout.dock!.sizePx}
                        session={activeDetailSession}
                        onOpenSource={activeDetailSession ? () => openSourceFor(activeDetailSession.header.sessionId) : undefined}
                        onMove={onMoveDock}
                        onResize={(px) => resizeDock(px, windowWidth)}
                        onResizeReset={() => resizeDock(360, windowWidth)}
                        onUnpin={closeDock}
                      />
                    )}

                    {/* Drag-drop zones (tasks.md 7.5): rendered only while a
                        panel drag is in progress (`AgentDeskDockDropZone`
                        itself no-ops otherwise), one per placement. */}
                    <AgentDeskDockDropZone
                      zone="right"
                      onDrop={onDockDrop}
                      label="Right"
                      className="absolute right-2 top-12 bottom-2 w-24"
                    />
                    <AgentDeskDockDropZone
                      zone="bottom"
                      onDrop={onDockDrop}
                      label="Bottom"
                      className="absolute inset-x-2 bottom-2 h-16"
                    />
                    <AgentDeskDockDropZone
                      zone="left-above"
                      onDrop={onDockDrop}
                      label="Left, above chats"
                      className="absolute left-2 top-12 h-20 w-24"
                    />
                    <AgentDeskDockDropZone
                      zone="left-below"
                      onDrop={onDockDrop}
                      label="Left, below chats"
                      className="absolute bottom-2 left-2 h-20 w-24"
                    />
                  </div>
                )}
              </div>
            </>
          )}
        </div>
      )}

      <ConfirmDialog
        open={resetConfirmOpen}
        onOpenChange={setResetConfirmOpen}
        title="Reset workspace layout?"
        description="This puts Split View, panel placement, and panel size back to their defaults. Your chats and messages are not affected."
        confirmLabel="Reset layout"
        onConfirm={() => {
          resetLayout()
          setResetConfirmOpen(false)
          toast.success('Workspace layout reset.')
        }}
      />
    </div>
  )
}
