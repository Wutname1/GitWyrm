import { useState } from 'react'
import { toast } from 'sonner'
import { ArrowUp, Paperclip, Sparkles } from 'lucide-react'
import { commands } from '@/lib/bindings'
import { unwrap, keys } from '@/lib/queryKeys'
import { useQueryClient } from '@tanstack/react-query'
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
import { ProviderControl } from './ProviderControl'
import { TeamShapeControl } from './TeamShapeControl'

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
 *      as its own toast rather than rolled back, since the message was
 *      genuinely saved.
 * `sending` gates only the Send affordance (disabled state + guard in
 * `canSendComposerDraft`), never the textarea itself, so the user's typing
 * is never dropped while a previous send is in flight.
 *
 * Exactly one action button -- Send. There is deliberately no separate
 * stop/icon-only button beside it; "Stop all" lives only in the Graph panel
 * header (tasks.md 6.5, `AgentGraphPanel`).
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
export function SessionComposer({ sessionId }: { sessionId: string | null }) {
  const qc = useQueryClient()
  const draft = useAgentDeskUiStore((s) => (sessionId ? (s.drafts[sessionId]?.text ?? '') : ''))
  const setDraftInStore = useAgentDeskUiStore((s) => s.setDraft)
  const clearDraft = useAgentDeskUiStore((s) => s.clearDraft)
  const setDraft = (text: string) => {
    if (sessionId) setDraftInStore(sessionId, text)
  }
  const [sending, setSending] = useState(false)
  const [mode, setMode] = useState<ComposerMode>('Auto')
  const [team, setTeam] = useState<ComposerTeam>('helpers')
  const [teamOpen, setTeamOpen] = useState(false)
  // `null` means "whatever the default is" -- deliberately not resolved to the
  // default tool's id, so a chat nobody gave a preference keeps following the
  // default if it ever changes.
  const [provider, setProvider] = useState<string | null>(null)
  const [providerOpen, setProviderOpen] = useState(false)

  const canSend = canSendComposerDraft({ draft, sessionId, sending })

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

        try {
          const startOutcome = unwrap(
            await commands.agentSessionStartExecution(
              sessionId,
              modeToExecutionMode(mode),
              teamToExecutionTeam(team),
              provider
            )
          )
          if (startOutcome.kind === 'started') {
            void qc.invalidateQueries({ queryKey: keys.agentSession(sessionId) })
          } else if (startOutcome.kind === 'alreadyRunning') {
            // R3.6: this is the case the reset audit called out by name --
            // "never claim the current run received a message when it did
            // not." There is no real steering channel into a live ACP
            // execution today (no queue the running process reads from
            // mid-turn); `agentSessionAppendUserMessage` above only wrote the
            // message into this session's own transcript file, and
            // `agentSessionStartExecution` refused to start a second engine
            // on top of the one already running. The message is saved and
            // visible, but the running agent has not seen it and will not
            // until its current turn ends and something starts a fresh
            // execution (the user sending another message once it is idle,
            // or the lead's own next turn picking it up if the provider
            // happens to poll the transcript -- neither of which this call
            // caused). Say exactly that, rather than implying delivery.
            toast.info('Saved for the next turn. The agent is still finishing its current one.', {
              description: 'It will see this message once it stops or you send it again after that.',
            })
          } else if (startOutcome.kind === 'sourceMissing') {
            toast.error('This chat needs its repository open to run.', { description: startOutcome.detail })
          } else if (startOutcome.kind === 'adapterUnsupported') {
            toast.error('That provider is not available right now.', { description: startOutcome.detail })
          } else if (startOutcome.kind === 'providerReconnect') {
            toast.error('Reconnect the provider to continue.', { description: startOutcome.detail })
          } else if (startOutcome.kind === 'notFound') {
            toast.error('This chat is gone. It may have been archived elsewhere.')
          } else if (startOutcome.kind === 'damaged') {
            toast.error('This chat file is damaged and could not start.', { description: startOutcome.reason })
          } else {
            toast.error('Your message was sent, but the agent could not start.', { description: startOutcome.kind })
          }
        } catch (e) {
          const message = describeError(e)
          log.error(`agent desk: could not start execution: ${message}`)
          toast.error('Your message was sent, but the agent could not start.', { description: message })
        }
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
    <div className="flex-none border-t border-border p-2">
      <div className="rounded-lg border border-border bg-panel2 p-1.5">
        <OperatingModeControl mode={mode} onChange={setMode} />

        <Textarea
          // Stable id so "New chat" can put the caret straight in here.
          // Focusing the surrounding wrapper only moved focus near the box,
          // leaving the user to click before typing.
          id={`agent-desk-composer-${sessionId ?? 'none'}`}
          data-agent-desk-composer
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          placeholder="Steer the lead or ask about the work…"
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

          <TeamShapeControl team={team} onChange={setTeam} open={teamOpen} onOpenChange={setTeamOpen} />

          {/* Whether this chat is read-only is answered by the backend, not
              worked out here: the rule depends on the session's intent and
              whether a Plan has started, and an earlier version derived it
              from the mode pill and disagreed with the engine's own gate. */}
          <ProviderControl
            sessionId={sessionId}
            provider={provider}
            onChange={setProvider}
            open={providerOpen}
            onOpenChange={setProviderOpen}
          />

          <span className="flex-1" />

          <button
            type="button"
            disabled={!canSend}
            onClick={() => void send()}
            className="flex flex-none items-center gap-1 rounded-md bg-primary px-2.5 py-1 text-2xs font-semibold text-primary-foreground disabled:cursor-not-allowed disabled:opacity-50"
          >
            {sending ? 'Sending…' : 'Send'}
            <ArrowUp size={12} />
          </button>
        </div>
      </div>
    </div>
  )
}
