import { useEffect, useRef, useState } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import { listen } from '@tauri-apps/api/event'
import { toast } from 'sonner'
import { commands, type CreateSessionRequest, type RepoInfo, type SelectDeskTarget } from '@/lib/bindings'
import { unwrap, keys } from '@/lib/queryKeys'
import { describeError, log } from '@/lib/log'
import { readWindowMode, type WindowMode } from '@/lib/windowMode'
import { AgentDeskTitleBar } from '@/components/domain/agent-desk/AgentDeskTitleBar'
import { SessionSidebar } from '@/components/domain/agent-desk/SessionSidebar'
import { ConversationPane } from '@/components/domain/agent-desk/ConversationPane'
import { OpenSpecEmbeddedDetail } from '@/components/domain/agent-desk/OpenSpecEmbeddedDetail'
import { useAgentSessionHeaders } from '@/hooks/useAgentSessions'
import { cn } from '@/lib/utils'
import { resolveAgentDeskShellState } from '@/views/agentDeskViewState'

type RightTab = 'context' | 'graph'
type CenterView = 'conversation' | 'openspec'

/** Matches the Rust side's `agent-desk://select-target` in `spec_desk.rs`. */
const SELECT_DESK_TARGET_EVENT = 'agent-desk://select-target'

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
 * The Agent Desk window: dense session navigation on the left, one active
 * conversation in the center, and Context/Graph/OpenSpec detail on the
 * right.
 *
 * Layout seam for the future workspace-layout package: everything below the
 * titlebar lives inside `.agent-desk-columns`, a single flex row of
 * `[sidebar][center][right]`. Adding a second `ConversationPane` (Split
 * View) or a dock on another edge means changing what sits in the center
 * slot and adding a sibling column here -- it does not require touching the
 * sidebar or titlebar. `ConversationPane` itself only ever receives a
 * `sessionId` and `isActive`; nothing in this file reaches into a pane's
 * internals or assumes there is exactly one.
 */
export function AgentDeskView() {
  const mode = useDeskTarget()
  const { repo, error } = useDeskRepo(mode.repoPath)
  const repoId = repo?.id ?? null
  const qc = useQueryClient()

  const [selectedSessionId, setSelectedSessionId] = useState<string | null>(null)
  const [rightTab, setRightTab] = useState<RightTab>('context')
  const [centerView, setCenterView] = useState<CenterView>('conversation')
  const [creating, setCreating] = useState(false)
  const composerFocusRef = useRef<HTMLDivElement | null>(null)

  // Rule #1: a retarget must be visibly apparent, not silent. Clearing the
  // selected session both (a) stops the previous repo's conversation from
  // staying on screen under the new repo's title/sidebar -- which would look
  // like data from two repositories got mixed together -- and (b) lets the
  // "land on the most recent session" effect below pick the new repo's own
  // most recent chat, the same way it already does on first paint.
  //
  // Two separate effects, deliberately: the first reacts to `mode.repoId`
  // alone so the reset happens the instant a retarget is detected, without
  // waiting on `useDeskRepo`'s async `openRepo` to resolve. The second reacts
  // to `repo` (which briefly goes back to `null` while the new repo opens,
  // then becomes the new `RepoInfo`) so the toast names the repo that was
  // actually just switched to, not the one being left.
  const previousRepoId = useRef(mode.repoId)
  const announceNextRepo = useRef(false)
  useEffect(() => {
    if (previousRepoId.current !== mode.repoId) {
      previousRepoId.current = mode.repoId
      announceNextRepo.current = true
      setSelectedSessionId(null)
      setCenterView('conversation')
    }
  }, [mode.repoId])

  useEffect(() => {
    if (repo && announceNextRepo.current) {
      announceNextRepo.current = false
      toast.info(`Switched to ${repo.name}`)
    }
  }, [repo])

  const filter = repoId
    ? {
        repoId,
        projectPath: null,
        states: [],
        sourceKinds: [],
        hasChangedFiles: null,
        archived: false,
        titleContains: null,
      }
    : {
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

  // Land on the most recent session automatically so the window is never
  // just an empty pane the first time it opens with sessions already saved.
  useEffect(() => {
    if (selectedSessionId == null && headers.length > 0) {
      setSelectedSessionId(headers[0].sessionId)
    }
  }, [headers, selectedSessionId])

  const onSelectSession = (sessionId: string) => {
    setSelectedSessionId(sessionId)
    // Rule #1: a click always produces a visible response. Selecting a
    // session both highlights the row (handled by SessionSidebar's
    // aria-current styling) and moves focus into the conversation so a
    // keyboard user lands somewhere useful, not on a stale focus target.
    composerFocusRef.current?.focus()
  }

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
        intent: 'ask',
      }
      const outcome = unwrap(await commands.agentSessionCreate(request))
      if (outcome.kind === 'created') {
        setSelectedSessionId(outcome.session.header.sessionId)
        void qc.invalidateQueries({ queryKey: keys.agentSessionsAll })
        toast.success('New chat started.')
      } else {
        toast.error('Could not start a new chat.', { description: outcome.kind })
      }
    } catch (e) {
      const message = describeError(e)
      log.error(`agent desk: could not create session: ${message}`)
      toast.error('Could not start a new chat.', { description: message })
    } finally {
      setCreating(false)
    }
  }

  const repoName = repo?.name ?? 'Loading…'

  const shellState = resolveAgentDeskShellState({
    hasRepoPath: mode.repoPath != null,
    repoError: mode.repoPath != null ? error : null,
    repoReady: repo != null,
    sessionsLoading,
    sessionCount: headers.length,
    hasSelection: selectedSessionId != null,
  })

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
            <SessionSidebar repoId={repoId} selectedId={selectedSessionId} onSelectSession={onSelectSession} onNewSession={() => void onNewChat()} />
          )}

          {centerView === 'openspec' ? (
            /* Task 2.3: keep current OpenSpec details/actions functional.
               Full-width so DeskDetail/DeskActionRail/DeskChangesList render
               at the same proportions they always have, rather than being
               squeezed into the 306px right rail meant for Context/Graph. */
            <OpenSpecEmbeddedDetail repoId={repo.id} repoPath={repo.path} />
          ) : (
            <>
              <div ref={composerFocusRef} tabIndex={-1} className="flex min-h-0 flex-1 flex-col outline-none">
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
                  <ConversationPane sessionId={selectedSessionId} isActive />
                )}
              </div>

              <div className="flex min-h-0 w-[306px] flex-none flex-col border-l border-border bg-panel">
                <div
                  role="tablist"
                  aria-label="Session details"
                  className="flex flex-none gap-3 border-b border-border px-3 pt-2"
                >
                  {(['context', 'graph'] as const).map((t) => (
                    <button
                      key={t}
                      role="tab"
                      aria-selected={rightTab === t}
                      onClick={() => setRightTab(t)}
                      className={cn(
                        '-mb-px border-b-2 pb-1.5 text-2xs font-semibold capitalize transition-colors',
                        rightTab === t
                          ? 'border-primary text-foreground'
                          : 'border-transparent text-sub hover:text-foreground'
                      )}
                    >
                      {t}
                    </button>
                  ))}
                </div>
                <div className="min-h-0 flex-1 overflow-y-auto p-3">
                  {rightTab === 'context' && (
                    <p className="text-2xs leading-relaxed text-muted-foreground">
                      Project, branch, and source details for the selected chat will appear here.
                    </p>
                  )}
                  {rightTab === 'graph' && (
                    <div className="flex h-full flex-col items-center justify-center gap-1 text-center">
                      <p className="text-xs font-semibold text-foreground">No agents running</p>
                      <p className="max-w-[16rem] text-2xs leading-relaxed text-muted-foreground">
                        Solo chats work alone. Plan drafts a graph and waits for you to start it.
                        Auto may start helpers on its own when it is useful.
                      </p>
                    </div>
                  )}
                </div>
              </div>
            </>
          )}
        </div>
      )}
    </div>
  )
}
