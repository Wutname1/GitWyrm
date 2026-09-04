import { useEffect, useMemo, useRef, useState } from 'react'
import { toast } from 'sonner'
import { ArrowUp, Paperclip, Sparkles, Square } from 'lucide-react'
import { commands, type AgentSessionHeader } from '@/lib/bindings'
import { unwrap, keys } from '@/lib/queryKeys'
import { explainStopOutcome, runIsActive } from '@/lib/agentDeskResult'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { describeError, log } from '@/lib/log'
import { Textarea } from '@/components/ui/textarea'
import { useAgentDeskUiStore } from '@/stores/agentDeskUiStore'
import {
  canSendComposerDraft,
  modeToExecutionMode,
  teamToExecutionTeam,
  type ComposerMode,
  type ComposerTeam,
} from '@/lib/agentDeskComposer'
import { OperatingModeControl } from './OperatingModeControl'
import { cn } from '@/lib/utils'
import { NewChatLanding } from './NewChatLanding'
import { ProviderControl } from './ProviderControl'
import { TeamShapeControl } from './TeamShapeControl'
import { useWorkspaceStore } from '@/stores/workspaceStore'
import { useOpenRepo } from '@/hooks/useRepoActions'
import {
  startFailureCardForError,
  startFailureCardForExecution,
  type StartFailureAction,
  type StartFailureCard as StartFailureCardModel,
} from '@/lib/agentDeskStartFailure'
import { StartFailureCard } from './StartFailureCard'
import type { ChatProjectChoice } from './NewChatLanding'

/**
 * The message composer: mode/team controls, the draft textarea, and Send.
 *
 * Owns the send sequence for tasks.md 6.3/6.4:
 *   1. Append the user's message (durable write; the query invalidation this
 *      triggers makes it show up in the transcript immediately -- Rule #1,
 *      "every action produces a visible response" -- *before* anything about
 *      starting the engine is even attempted).
 *   2. Only once the append has durably landed, ask the backend to start (or
 *      continue) an execution in the chosen mode/team. A failure here still
 *      leaves the user's message visible in the transcript; it is reported
 *      as a card above the composer (`StartFailureCard`, with Try again and
 *      the fix for that kind of failure) rather than rolled back, since the
 *      message was genuinely saved.
 * `sending` gates only the Send affordance (disabled state + guard in
 * `canSendComposerDraft`), never the textarea itself, so the user's typing
 * is never dropped while a previous send is in flight.
 *
 * One action button, which is Send while the chat is idle and Stop while a
 * run is going. Stop used to live ONLY in the Graph panel header, and that
 * panel is not offered for a solo chat (`sessionHasGraph` is false when there
 * is no helper and no proposed graph) -- so the single highest-stakes control
 * on the surface was unreachable for the simpler of the two team shapes,
 * while an agent was editing files. The Graph panel keeps its per-node Stop
 * and Stop all for team runs; this is the one that is always reachable.
 *
 * Draft text is session-scoped through `useAgentDeskUiStore` (task group 3:
 * "Session-scoped drafts") rather than local `useState` -- replacing this
 * pane's session (a sidebar click, Split View swapping which session is
 * active) must not discard whatever the user was mid-typing, and switching
 * back to that session must restore it verbatim. `getDraft`/`setDraft` are
 * keyed by `sessionId`, so two panes showing two different sessions keep
 * fully independent drafts for free -- there is nothing pane-local to keep
 * in sync.
 */
export function SessionComposer({
  sessionId,
  header,
  /** True while this chat has nothing in it yet. */
  isEmpty = false,
}: {
  sessionId: string | null
  header: AgentSessionHeader | null
  isEmpty?: boolean
}) {
  const qc = useQueryClient()
  const draft = useAgentDeskUiStore((s) => (sessionId ? (s.drafts[sessionId]?.text ?? '') : ''))
  const setDraftInStore = useAgentDeskUiStore((s) => s.setDraft)
  const clearDraft = useAgentDeskUiStore((s) => s.clearDraft)
  const setDraft = (text: string) => {
    if (sessionId) setDraftInStore(sessionId, text)
  }
  const [sending, setSending] = useState(false)
  const [mode, setMode] = useState<ComposerMode>(
    header?.preferredMode === 'Ask' || header?.preferredMode === 'Plan' ? header.preferredMode : 'Auto'
  )
  const [team, setTeam] = useState<ComposerTeam>(header?.preferredTeam === 'solo' ? 'solo' : 'helpers')
  const [teamOpen, setTeamOpen] = useState(false)
  // `null` means "whatever the default is" -- deliberately not resolved to the
  // default tool's id, so a chat nobody gave a preference keeps following the
  // default if it ever changes.
  const [provider, setProvider] = useState<string | null>(header?.preferredProvider ?? null)
  const [providerOpen, setProviderOpen] = useState(false)
  // Source-kickoffs 2.4/5.3: the last failed start stays on screen as a card
  // until the person acts on it or closes it. A toast alone was gone before
  // anyone who stepped away could read it, leaving a saved message and no
  // way forward. `retrying` gates the card's own buttons while a retry or
  // project open is in flight.
  const [startFailure, setStartFailure] = useState<StartFailureCardModel | null>(null)
  const [retrying, setRetrying] = useState(false)
  const openRepo = useOpenRepo()
  const preferenceWrite = useRef<Promise<void>>(Promise.resolve())
  // Resolved to the tool's real name rather than its id: the landing would
  // otherwise read "copilot" where the rest of the app says "GitHub Copilot".
  const providerNames = useQuery({
    queryKey: keys.agentProviders(sessionId),
    queryFn: async () => unwrap(await commands.agentProvidersList(sessionId)),
    enabled: isEmpty,
  })
  const providerLabel =
    providerNames.data?.providers.find((p) => p.id === provider)?.displayName ??
    (provider ?? 'Default AI')

  const openRepos = useWorkspaceStore((s) => s.openRepos)
  const recents = useWorkspaceStore((s) => s.recents)
  const projects = useMemo<ChatProjectChoice[]>(() => {
    const byPath = new Map<string, ChatProjectChoice>()
    if (header) byPath.set(header.repoPath.toLowerCase(), { name: header.repoName, path: header.repoPath })
    for (const repo of openRepos) byPath.set(repo.path.toLowerCase(), { name: repo.name, path: repo.path })
    for (const recent of recents) {
      if (!byPath.has(recent.path.toLowerCase())) byPath.set(recent.path.toLowerCase(), recent)
    }
    return [...byPath.values()]
  }, [header, openRepos, recents])

  // Controls are chat state, not pane state. A pane switch must restore the
  // incoming chat's saved choices instead of carrying the previous chat's
  // authority and provider across with the mounted composer component.
  useEffect(() => {
    setMode(header?.preferredMode === 'Ask' || header?.preferredMode === 'Plan' ? header.preferredMode : 'Auto')
    setTeam(header?.preferredTeam === 'solo' ? 'solo' : 'helpers')
    setProvider(header?.preferredProvider ?? null)
  }, [header?.sessionId, header?.preferredMode, header?.preferredTeam, header?.preferredProvider])

  // A failure card belongs to the chat it happened in. Swapping this pane
  // to another chat must not carry it across.
  useEffect(() => {
    setStartFailure(null)
  }, [sessionId])

  const savePreferences = (nextMode: ComposerMode, nextTeam: ComposerTeam, nextProvider: string | null) => {
    if (!sessionId) return
    const targetSessionId = sessionId
    // Keep rapid clicks in click order. Otherwise a slow first disk write can
    // land after the user's newer choice and silently restore the old value.
    preferenceWrite.current = preferenceWrite.current
      .then(async () => {
        const outcome = unwrap(
          await commands.agentSessionSetPreferences(targetSessionId, nextMode, nextTeam, nextProvider)
        )
        if (outcome.kind === 'updated') {
          void qc.invalidateQueries({ queryKey: keys.agentSession(targetSessionId) })
          void qc.invalidateQueries({ queryKey: keys.agentSessionsAll })
        } else {
          throw new Error(outcome.kind)
        }
      })
      .catch((e) => {
        toast.error('Could not save that chat setting.', { description: describeError(e) })
      })
  }

  const changeMode = (next: ComposerMode) => {
    setMode(next)
    savePreferences(next, team, provider)
  }
  const changeTeam = (next: ComposerTeam) => {
    setTeam(next)
    savePreferences(mode, next, provider)
  }
  const changeProvider = (next: string | null) => {
    setProvider(next)
    savePreferences(mode, team, next)
  }

  const changeProject = async (project: ChatProjectChoice) => {
    if (!sessionId || !header || project.path.toLowerCase() === header.repoPath.toLowerCase()) return
    try {
      const target = unwrap(await commands.openRepo(project.path))
      const outcome = unwrap(
        await commands.agentSessionSetProject(sessionId, target.id, target.path, target.name)
      )
      if (outcome.kind === 'moved') {
        void qc.invalidateQueries({ queryKey: keys.agentSession(sessionId) })
        void qc.invalidateQueries({ queryKey: keys.agentSessionsAll })
        toast.success(`This chat now uses ${target.name}.`)
      } else if (outcome.kind === 'alreadyStarted') {
        // Said plainly rather than reported as a failure: nothing broke, the
        // chat simply has work in it now and its project is settled.
        toast.info('This chat has already started, so it stays with its project.', {
          description: 'Start a new chat to work in another project.',
        })
      } else if (outcome.kind === 'notFound') {
        toast.error('This chat is gone.')
      } else {
        toast.error('Could not change this chat’s project.', { description: outcome.detail })
      }
    } catch (e) {
      toast.error('Could not open that project.', { description: describeError(e) })
    }
  }

  const canSend = canSendComposerDraft({ draft, sessionId, sending })
  // A run is going, so the action button offers the way out of it.
  const running = runIsActive(header?.state)
  const [stopping, setStopping] = useState(false)

  const stopRun = async () => {
    if (!sessionId || stopping) return
    setStopping(true)
    try {
      // Same outcome handling as the Graph panel's Stop all, so the two
      // controls cannot report the same event differently.
      const outcome = unwrap(await commands.agentSessionStopExecution(sessionId, { kind: 'all' }))
      // One explainer for all three Stop call sites. Each branched on
      // `stopped.length` alone and ignored `timed_out`, so a hung agent --
      // stopped: [], timed_out: [id] -- reported "Nothing was running."
      if (outcome.kind === 'stopped') {
        void qc.invalidateQueries({ queryKey: keys.agentSession(sessionId) })
      }
      const { message, ok } = explainStopOutcome(outcome)
      if (ok) toast.success(message)
      else toast.error(message)
    } catch (e) {
      const message = describeError(e)
      log.error(`agent desk: could not stop from the composer: ${message}`)
      toast.error('Could not stop this chat.', { description: message })
    } finally {
      setStopping(false)
    }
  }

  /**
   * The start step, on its own so Send and the failure card's Try again run
   * the exact same code. Uses the composer's current mode/team/provider:
   * those are the chat's saved choices, so a retry re-runs "the same" start
   * unless the person changed one on purpose (picking another AI tool from
   * the card, say), in which case the change is what they want applied.
   *
   * Never throws. A typed refusal or a thrown error becomes the card; a
   * success clears it.
   */
  const startExecution = async (targetSessionId: string): Promise<void> => {
    try {
      const startOutcome = unwrap(
        await commands.agentSessionStartExecution(
          targetSessionId,
          modeToExecutionMode(mode),
          teamToExecutionTeam(team),
          provider
        )
      )
      if (startOutcome.kind === 'started') {
        setStartFailure(null)
        void qc.invalidateQueries({ queryKey: keys.agentSession(targetSessionId) })
        return
      }
      if (startOutcome.kind === 'alreadyRunning') {
        // R3.6: this is the case the reset audit called out by name --
        // "never claim the current run received a message when it did
        // not." There is no real steering channel into a live ACP
        // execution today (no queue the running process reads from
        // mid-turn); `agentSessionAppendUserMessage` only wrote the
        // message into this session's own transcript file, and
        // `agentSessionStartExecution` refused to start a second engine
        // on top of the one already running. The message is saved and
        // visible, and the next turn starts from the queued follow-up on
        // its own once the current one ends. Say exactly that, rather
        // than implying delivery. Not a failure, so no card.
        setStartFailure(null)
        toast.info('Saved. The agent will pick this up as soon as it finishes its current turn.')
        return
      }
      const card = startFailureCardForExecution(startOutcome)
      if (card) {
        setStartFailure(card)
        return
      }
      // The chat itself is gone or unreadable: nothing on the card could
      // help, so these stay one-shot toasts.
      if (startOutcome.kind === 'notFound') {
        toast.error('This chat is gone. It may have been archived elsewhere.')
      } else if (startOutcome.kind === 'damaged') {
        toast.error('This chat file is damaged and could not start.', { description: startOutcome.reason })
      }
    } catch (e) {
      const message = describeError(e)
      log.error(`agent desk: could not start execution: ${message}`)
      setStartFailure(startFailureCardForError(message))
    }
  }

  const tryStartAgain = async () => {
    if (!sessionId || retrying) return
    setRetrying(true)
    try {
      await startExecution(sessionId)
    } finally {
      setRetrying(false)
    }
  }

  /**
   * "Open project" on a sourceMissing card: opens the chat's repository as a
   * tab (the same hook the sidebar and worktree list use, so it lands in the
   * workspace store and toasts "Opened X"), then runs the start straight
   * away. Stopping after the open would leave the person with the same card
   * and one more click to make; if the start fails again the card updates.
   */
  const openProjectAndStart = async () => {
    if (!sessionId || !header || retrying) return
    setRetrying(true)
    try {
      await openRepo.mutateAsync(header.repoPath)
      await startExecution(sessionId)
    } catch {
      // `useOpenRepo` already toasted the reason. The card stays so the
      // person can try again once the folder is reachable.
    } finally {
      setRetrying(false)
    }
  }

  const onFailureAction = (action: StartFailureAction) => {
    if (action === 'tryAgain') void tryStartAgain()
    else if (action === 'openProject') void openProjectAndStart()
    else if (action === 'pickProvider') setProviderOpen(true)
  }

  const send = async () => {
    if (!canSend || !sessionId) return
    setSending(true)
    const content = draft.trim()
    try {
      const outcome = unwrap(await commands.agentSessionAppendUserMessage(sessionId, content, []))
      if (outcome.kind === 'appended') {
        // The message is durably saved and the transcript query is about to
        // reflect it -- clear the draft and invalidate before touching
        // execution, so a failure below never hides that the send worked.
        // tasks.md 3.5: clear only *this* session's draft, and only now that
        // Send has actually persisted its user event.
        clearDraft(sessionId)
        void qc.invalidateQueries({ queryKey: keys.agentSession(sessionId) })
        void qc.invalidateQueries({ queryKey: keys.agentSessionsAll })

        await startExecution(sessionId)
      } else if (outcome.kind === 'notFound') {
        toast.error('This chat is gone. It may have been archived elsewhere.')
      } else if (outcome.kind === 'damaged') {
        toast.error('This chat file is damaged and could not accept the message.', {
          description: outcome.reason,
        })
      } else {
        toast.error('The message could not be saved.', { description: outcome.kind })
      }
    } catch (e) {
      const message = describeError(e)
      log.error(`agent desk: could not send message: ${message}`)
      toast.error('Could not send that message.', { description: message })
    } finally {
      setSending(false)
    }
  }

  return (
    <div className={cn(isEmpty ? 'flex min-h-0 flex-1 flex-col' : 'flex-none', 'border-t border-border p-2')}>
      {/* Before there is anything to read, the choices that shape the run get
          the space instead of hiding as chips under the box. They edit the
          same state the compact controls do, so nothing is lost when this
          gives way to the transcript. */}
      {isEmpty && (
        <NewChatLanding
          mode={mode}
          onModeChange={changeMode}
          team={team}
          onTeamChange={changeTeam}
          providerLabel={providerLabel}
          onOpenProviderPicker={() => setProviderOpen(true)}
          projectPath={header?.repoPath ?? ''}
          projectName={header?.repoName ?? 'Current project'}
          projects={projects}
          onProjectChange={(project) => void changeProject(project)}
          source={header?.source ?? null}
        />
      )}
      {startFailure && (
        <div className="mb-1.5">
          <StartFailureCard
            card={startFailure}
            busy={retrying || sending}
            onAction={onFailureAction}
            onDismiss={() => setStartFailure(null)}
          />
        </div>
      )}
      <div className="rounded-lg border border-border bg-panel2 p-1.5">
        <OperatingModeControl mode={mode} onChange={changeMode} />

        <Textarea
          // Stable id so "New chat" can put the caret straight in here.
          // Focusing the surrounding wrapper only moved focus near the box,
          // leaving the user to click before typing.
          id={`agent-desk-composer-${sessionId ?? 'none'}`}
          data-agent-desk-composer
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          placeholder="Say what to do next, or ask about the work…"
          aria-label="Message the lead agent"
          rows={2}
          className="resize-none border-0 bg-transparent px-1 py-1 text-xs shadow-none focus-visible:ring-0"
          onKeyDown={(e) => {
            if (e.key === 'Enter' && !e.shiftKey) {
              e.preventDefault()
              void send()
            }
          }}
        />

        <div className="flex items-center gap-1.5 px-0.5 pt-1">
          {/* Disabled rather than toast-on-click: a button that looks live and
              only apologises teaches the user that controls here are decorative.
              There is no attachment picker behind this yet, so it says so. */}
          <button
            type="button"
            disabled
            aria-label="Attach context (not available yet)"
            title="Attaching files and notes is not available yet"
            className="flex h-6 w-6 flex-none items-center justify-center rounded text-muted-foreground opacity-40"
          >
            <Paperclip size={13} />
          </button>
          {/* The mockup names a lead agent ("Sol"), but nothing produces that
              name -- the session carries no agent identity, and hardcoding one
              claims something untrue about whichever provider is really
              answering. State the shape of the team instead, which is real. */}
          <span className="flex flex-none items-center gap-1 text-2xs text-muted-foreground">
            <Sparkles size={12} className="text-accent-text" />
            {team === 'solo' ? 'One agent' : 'A lead agent, up to 3 helpers'}
          </span>

          <TeamShapeControl team={team} onChange={changeTeam} open={teamOpen} onOpenChange={setTeamOpen} />

          {/* Whether this chat is read-only is answered by the backend, not
              worked out here: the rule depends on the session's intent and
              whether a Plan has started, and an earlier version derived it
              from the mode pill and disagreed with the engine's own gate. */}
          <ProviderControl
            sessionId={sessionId}
            provider={provider}
            onChange={changeProvider}
            open={providerOpen}
            onOpenChange={setProviderOpen}
          />

          <span className="flex-1" />

          {running ? (
            <button
              type="button"
              disabled={stopping}
              onClick={() => void stopRun()}
              className="flex flex-none items-center gap-1 rounded-md border border-destructive/50 px-2.5 py-1 text-2xs font-semibold text-destructive hover:bg-destructive/10 disabled:cursor-not-allowed disabled:opacity-50"
            >
              {stopping ? 'Stopping…' : 'Stop'}
              <Square size={11} aria-hidden />
            </button>
          ) : (
            <button
              type="button"
              disabled={!canSend}
              onClick={() => void send()}
              className="flex flex-none items-center gap-1 rounded-md bg-primary px-2.5 py-1 text-2xs font-semibold text-primary-foreground disabled:cursor-not-allowed disabled:opacity-50"
            >
              {sending ? 'Sending…' : 'Send'}
              <ArrowUp size={12} />
            </button>
          )}
        </div>
      </div>
    </div>
  )
}
