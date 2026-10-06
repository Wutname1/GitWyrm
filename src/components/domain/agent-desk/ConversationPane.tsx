import { useEffect, useMemo, useRef, useState } from 'react'
import { Download, ExternalLink, FileDiff, GitFork, ListChecks, TriangleAlert } from 'lucide-react'
import { toast } from 'sonner'
import { commands, type MessageTarget, type SessionMessage } from '@/lib/bindings'
import { useAgentSession } from '@/hooks/useAgentSessions'
import { useMessageTargetNav, type MessageTargetNav } from '@/hooks/useMessageTargetNav'
import { cn } from '@/lib/utils'
import { Markdown } from '@/components/ui/markdown'
import { DisabledHint } from '@/components/ui/tooltip'
import { isNearBottom } from '@/lib/agentDeskScroll'
import { computeRailTicks, currentMessageForScroll, userMessagesForRail } from '@/lib/agentDeskRail'
import { groupEventStacks, type EventStackGroup } from '@/lib/agentDeskEvents'
import { parsePlanChecklist } from '@/lib/agentDeskPlan'
import { displayText, foldThoughtSummaries } from '@/lib/agentDeskTranscript'

import { gateAnswerNoteFor, gateBody, gateOptions, gateRequestOf, gateSummary, type GateOption } from '@/lib/agentDeskGate'
import { runActivityLabel, runIsActive, runStoppedBadly, runStoppedBadlyLabel, shouldShowResultPanel } from '@/lib/agentDeskResult'
import { adapterDisplayName } from '@/lib/agentImportDisplay'
import { log, describeError } from '@/lib/log'
import { useAgentDeskUiStore } from '@/stores/agentDeskUiStore'
import { SessionSourceBanner } from './SessionSourceBanner'
import { SessionComposer } from './SessionComposer'
import { ThoughtBlock } from './ThoughtBlock'
import { describeImportedAt, formatClock } from '@/lib/agentDeskSources'
import { PlanChecklist } from './PlanChecklist'
import { EventStack } from './EventStack'
import { MessageHistoryRail } from './MessageHistoryRail'
import { MessageActions } from './MessageActions'
import { ResultReviewPanel } from './ResultReviewPanel'

/** Plain-language label for each message kind, in the order they can appear. */
function kindLabel(kind: SessionMessage['kind']): string {
  switch (kind) {
    case 'user':
      return 'You'
    case 'assistant':
      return 'Lead'
    case 'thoughtSummary':
      return 'Thinking'
    case 'tool':
      return 'Tool activity'
    case 'approval':
      return 'Needs your approval'
    case 'system':
      return 'System note'
    case 'result':
      return 'Result'
  }
}

/**
 * Short avatar initials: "You" for the user, otherwise the first letters of
 * whoever produced the message.
 *
 * `message.provider` is set only on imported messages, and holds an adapter
 * id ("vscode-copilot"). Slicing it raw gave "VS"/"OP"/"CL", and every native
 * message -- the overwhelming majority -- fell through to `'Lead'` and
 * avatared as "LE": a truncation artifact, not a name, on the most-repeated
 * glyph in the product.
 *
 * Resolving the id first means the initials come from a real name, and native
 * messages say "AI" rather than two letters of a word nobody chose.
 */
function avatarInitials(message: SessionMessage): string {
  if (message.role === 'user') return 'You'
  if (!message.provider) return 'AI'
  return adapterDisplayName(message.provider).slice(0, 2)
}

/**
 * One link to wherever a message points (tasks.md 4.4). `source` goes
 * through `onOpenSource` (shared with the source banner); every other kind
 * goes through `useMessageTargetNav`, which opens the file's diff in the
 * main window, highlights the node in the graph dock, or switches to the
 * Spec view. A target the app genuinely cannot show renders disabled with
 * the reason in a hover hint, never as a dead-looking live button -- see
 * `resolveMessageTarget`.
 */
function MessageTargetLink({
  target,
  messageExecutionId,
  targetNav,
  onOpenSource,
}: {
  target: MessageTarget
  messageExecutionId: string | null
  targetNav: MessageTargetNav
  onOpenSource?: () => void
}) {
  const resolved = targetNav.resolve(target, messageExecutionId)
  if (resolved.kind === 'source') {
    return (
      <button
        type="button"
        onClick={onOpenSource}
        disabled={!onOpenSource}
        className="inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-2xs font-semibold text-accent-text hover:bg-soft disabled:cursor-not-allowed disabled:opacity-50"
      >
        <ExternalLink size={11} />
        {resolved.label}
      </button>
    )
  }
  if (resolved.kind === 'unavailable') {
    return (
      <DisabledHint disabled reason={resolved.reason}>
        <button
          type="button"
          disabled
          className="inline-flex max-w-full items-center gap-1 truncate rounded px-1.5 py-0.5 text-2xs font-medium text-muted-foreground disabled:cursor-not-allowed"
        >
          <Download size={11} className="flex-none rotate-180" aria-hidden />
          <span className="truncate">{resolved.label}</span>
        </button>
      </DisabledHint>
    )
  }
  const Icon = resolved.kind === 'graphNode' ? GitFork : resolved.kind === 'openSpecTask' ? ListChecks : FileDiff
  return (
    <button
      type="button"
      onClick={() => targetNav.open(resolved)}
      title={resolved.label}
      className="inline-flex max-w-full items-center gap-1 truncate rounded px-1.5 py-0.5 text-2xs font-semibold text-accent-text hover:bg-soft"
    >
      <Icon size={11} className="flex-none" aria-hidden />
      <span className="truncate">{resolved.label}</span>
    </button>
  )
}

/**
 * A message brought in from another chat client (tasks.md 4.2's "imported"
 * item). `import` on `SessionMessage` is not a `MessageKind` -- it is
 * `ImportProvenance`, present only on messages an adapter brought in from
 * outside GitWyrm -- so it is rendered as a decoration on top of whichever
 * kind the message actually is, not as a seventh kind of its own. This is
 * what stops an imported message from ever being presented as native
 * output, per the field's own doc comment in `bindings.ts`.
 */
function ImportedBadge({ adapterId, importedAt }: { adapterId: string; importedAt: string }) {
  // When it arrived here, which is not when it was written. Without this an
  // imported message dated last Tuesday reads as native history from last
  // Tuesday; `importedAt` exists precisely to keep those two facts apart.
  const arrived = describeImportedAt(importedAt)
  const from = `Imported from ${adapterDisplayName(adapterId)}`
  return (
    <span
      className="inline-flex items-center gap-1 rounded-full bg-panel3 px-1.5 py-px text-2xs font-semibold uppercase tracking-wide text-muted-foreground"
      title={arrived ? `${from} · ${arrived}` : from}
    >
      <Download size={9} aria-hidden />
      Imported
    </span>
  )
}

/**
 * Gap 4 of the 2026-08-21 implementation reset: `agent_session_answer_gate`
 * existed with no caller anywhere in the frontend, so an Auto run that hit a
 * destructive-action approval hung with no visible way forward -- the
 * transcript showed the amber "Needs your approval" row but nothing on it
 * could be clicked. This renders the three real answers (`gateOptions`) for
 * the specific request (`gateSummary`), and calls
 * `agent_session_answer_gate` with the message's own `executionId` -- R6.6's
 * "keyed by session, execution, and gate ID" is why this reads
 * `message.executionId` rather than the session's `activeExecutionId`: a
 * helper's gate must be answered against the helper's own execution, not
 * whichever execution happens to be the session's lead track right now.
 *
 * Every click gives visible feedback (Rule #1): a toast confirms the answer
 * was sent, or explains why it could not be (the run may have already ended
 * or been answered elsewhere -- `NoLiveGate` is not an error, just nothing
 * left to answer). The three buttons disable together once any one is
 * clicked, so a second click cannot send a second, conflicting answer to a
 * gate that already got one -- there is no confirmation dialog and no
 * type-to-confirm text box (Rule #3): the three labeled buttons already say
 * exactly what each one does.
 */
function GateApprovalControls({ sessionId, message }: { sessionId: string; message: SessionMessage }) {
  const [state, setState] = useState<'idle' | 'sending' | 'sent'>('idle')
  const request = useMemo(() => gateRequestOf(message), [message])
  if (!request || !message.executionId) return null

  async function answer(option: GateOption) {
    setState('sending')
    try {
      const result = await commands.agentSessionAnswerGate(sessionId, message.executionId!, option.answer)
      if (result.status !== 'ok') {
        toast.error('Could not send that answer. Try again.')
        setState('idle')
        return
      }
      if (result.data.kind === 'noLiveGate') {
        toast('This request is no longer waiting for an answer.', {
          description: 'The run may have already finished, stopped, or been answered.',
        })
      } else {
        toast.success('Sent.')
      }
      setState('sent')
    } catch (e) {
      log.error(`answer gate failed: ${describeError(e)}`)
      toast.error('Something went wrong sending that answer.')
      setState('idle')
    }
  }

  return (
    <div className="mt-1.5 flex flex-wrap items-center gap-1.5">
      <p className="w-full text-2xs font-medium text-foreground">{gateSummary(request)}</p>
      {/* The consequence, not just the ask. This used to show only the
          one-line summary, so the warnings that exist precisely because undo
          cannot help -- a command that reaches outside the project, or one
          that sends work off this machine where other people can see it --
          never reached the person actually answering the gate. */}
      <p className="w-full text-2xs leading-relaxed text-muted-foreground">{gateBody(request)}</p>
      {gateOptions().map((option) => (
        <button
          key={option.answer}
          type="button"
          onClick={() => void answer(option)}
          disabled={state !== 'idle'}
          className={cn(
            'rounded-md border px-2.5 py-1 text-2xs font-medium disabled:cursor-not-allowed disabled:opacity-50',
            option.answer === 'allowOnce' && 'border-primary bg-primary text-primary-foreground hover:bg-primary/90',
            option.answer === 'findAnotherWay' && 'border-border bg-panel2 text-foreground hover:bg-panel3',
            option.answer === 'stopRun' && 'border-destructive/40 text-destructive hover:bg-destructive/10'
          )}
        >
          {option.label}
        </button>
      ))}
      {state === 'sent' && <span className="text-2xs text-muted-foreground">Answer sent.</span>}
    </div>
  )
}

function MessageRow({
  sessionId,
  message,
  flash,
  targetNav,
  onOpenSource,
  thought,
  onEdit,
  answeredNote,
}: {
  sessionId: string
  message: SessionMessage
  /**
   * The recorded answer, when this approval has already been decided.
   *
   * Passed in rather than derived here because the whole message list is the
   * evidence, and this row only sees itself.
   */
  answeredNote?: string | null
  flash: boolean
  targetNav: MessageTargetNav
  onOpenSource?: () => void
  /**
   * A `thoughtSummary` message folded into this row, per
   * `foldThoughtSummaries` -- rendered inline above the body as a
   * `ThoughtBlock`, matching the mockup's `.ag-thought` sitting inside the
   * same `.ag-message` as the reply it explains.
   */
  thought?: SessionMessage
  /** Copy/Edit controls for a user message -- see `MessageActions`. Omitted (no controls rendered) for non-user rows. */
  onEdit?: (text: string) => void
}) {
  const isUser = message.role === 'user'
  // "Needs your approval" and tool activity get a visibly different treatment
  // from a plain chat bubble -- an approval in particular must never look
  // like an ordinary line of text the reader can skim past.
  const isApproval = message.kind === 'approval'
  const isTool = message.kind === 'tool'
  // Plan checklist rows (tasks.md's structured Plan block): parsed from the
  // message's own plain text via the lead's Markdown checklist convention --
  // see `src/lib/agentDeskPlan.ts`. A message with no checklist lines parses
  // to an empty list, so most rows never mount a `PlanChecklist` at all.
  const planRows = useMemo(
    () => (message.kind === 'assistant' || message.kind === 'result' ? parsePlanChecklist(message.plainContent) : []),
    [message.kind, message.plainContent]
  )
  return (
    <article
      id={`agent-desk-message-${message.messageId}`}
      data-message-id={message.messageId}
      tabIndex={-1}
      className={cn(
        // `select-text` at the message level, not just on the body: `body` sets
        // `user-select: none` (src/index.css), so without this the sender, the
        // timestamp and any thought/plan rows are unselectable and dragging
        // across a whole message copies only part of it.
        'group relative grid select-text grid-cols-[27px_minmax(0,1fr)] gap-2.5 rounded-md px-2 py-1.5 outline-none transition-colors duration-500 motion-reduce:transition-none',
        isApproval && 'border border-[color-mix(in_srgb,var(--gw-amber)_45%,transparent)] bg-[color-mix(in_srgb,var(--gw-amber)_8%,transparent)]',
        flash && 'bg-[color-mix(in_srgb,var(--gw-accent)_18%,transparent)]'
      )}
    >
      <span
        className={cn(
          'flex h-[27px] w-[27px] flex-none items-center justify-center rounded-full text-2xs font-bold uppercase',
          isUser ? 'bg-panel3 text-sub' : isTool ? 'bg-panel3 text-sub' : 'bg-soft text-accent-text'
        )}
        aria-hidden
      >
        {avatarInitials(message)}
      </span>
      {isUser && onEdit && <MessageActions message={message} onEdit={onEdit} className="absolute right-2 top-1.5" />}
      <div className="min-w-0">
        <div className="mb-1 flex flex-wrap items-center gap-1.5 text-2xs font-semibold text-foreground">
          <span className={cn(isApproval && 'text-[var(--gw-amber)]')}>{kindLabel(message.kind)}</span>
          {/* The stored value is an adapter id ("vscode-copilot"), so an imported
              chat bylined every message with an internal token while the badge
              beside it resolved the same string to a real name. */}
          {message.provider && (
            <span className="font-normal text-muted-foreground">{adapterDisplayName(message.provider)}</span>
          )}
          <span className="font-normal text-muted-foreground">{formatClock(message.timestamp)}</span>
          {message.import && <ImportedBadge adapterId={message.import.adapterId} importedAt={message.import.importedAt} />}
        </div>
        {thought && (
          <ThoughtBlock text={thought.plainContent} variant={message.kind === 'result' ? 'reviewing' : 'thinking'} />
        )}
        <div className="text-xs leading-relaxed text-foreground">
          {(() => {
            const { text, isMarkdown } = displayText(message)
            return isMarkdown ? (
              <Markdown text={text} />
            ) : (
              <p className="select-text whitespace-pre-wrap">{text}</p>
            )
          })()}
        </div>
        {planRows.length > 0 && (
          <PlanChecklist rows={planRows} label={message.kind === 'result' ? 'Review findings' : 'Agent plan'} />
        )}
        {isApproval &&
          (answeredNote ? (
            // Settled. The buttons used to come back live after any remount,
            // because "sent" lived only in component state -- so a decision
            // already made re-offered itself and clicking was then refused.
            <p className="mt-1.5 text-2xs font-medium text-muted-foreground">{answeredNote}</p>
          ) : (
            <GateApprovalControls sessionId={sessionId} message={message} />
          ))}
        {message.targets.length > 0 && (
          <div className="mt-1 flex flex-wrap items-center gap-1">
            {message.targets.map((target, i) => (
              <MessageTargetLink
                key={`${message.messageId}-target-${i}`}
                target={target}
                messageExecutionId={message.executionId}
                targetNav={targetNav}
                onOpenSource={onOpenSource}
              />
            ))}
          </div>
        )}
      </div>
    </article>
  )
}

/** A standalone thought block for a `thoughtSummary` message not folded into a later reply -- see `foldThoughtSummaries`. */
function StandaloneThoughtRow({ message }: { message: SessionMessage }) {
  return (
    <article
      id={`agent-desk-message-${message.messageId}`}
      data-message-id={message.messageId}
      tabIndex={-1}
      className="grid grid-cols-[27px_minmax(0,1fr)] gap-2.5 rounded-md px-2 py-1.5 outline-none"
    >
      <span
        className="flex h-[27px] w-[27px] flex-none items-center justify-center rounded-full bg-soft text-2xs font-bold uppercase text-accent-text"
        aria-hidden
      >
        {avatarInitials(message)}
      </span>
      <div className="min-w-0">
        <div className="mb-1 flex flex-wrap items-center gap-1.5 text-2xs font-semibold text-foreground">
          <span>{kindLabel(message.kind)}</span>
          {/* The stored value is an adapter id ("vscode-copilot"), so an imported
              chat bylined every message with an internal token while the badge
              beside it resolved the same string to a real name. */}
          {message.provider && (
            <span className="font-normal text-muted-foreground">{adapterDisplayName(message.provider)}</span>
          )}
          <span className="font-normal text-muted-foreground">{formatClock(message.timestamp)}</span>
        </div>
        <ThoughtBlock text={message.plainContent} />
      </div>
    </article>
  )
}

/**
 * One conversation surface: source banner, transcript with a message rail,
 * and the mode/team composer for a single session.
 *
 * Deliberately takes `sessionId` and `isActive` as props rather than reading
 * a global "current session" -- the workspace-layout package will mount a
 * second one of these side by side, and each pane owns only the session id
 * it was given. Selecting a different session in the sidebar is expected to
 * call back up through `onSelectSession` rather than writing here directly,
 * so a future layout can decide which pane a selection lands in.
 */
export interface ConversationPaneProps {
  sessionId: string | null
  isActive: boolean
  /** Ref the pane's header region so a future popover can anchor to it. */
  headerAnchorRef?: React.RefObject<HTMLDivElement | null>
  /** Rendered into the pane header's slot, e.g. future Source/Context/Graph buttons. */
  headerSlot?: React.ReactNode
  /**
   * Opens the live source (issue/PR/OpenSpec item/etc.) this chat started
   * from, or that a `source`-kind message target points back to. No caller
   * wires this today -- Agent Desk is a standalone window with no bridge
   * into the main window's issue/PR/OpenSpec surfaces yet (tasks.md 4.4) --
   * so both the source banner and any `source` message-target link render
   * disabled until one is passed in.
   */
  onOpenSource?: () => void
  /**
   * Whether the large source bar above the transcript is shown (tasks.md
   * 8.1/8.2). New prop rather than reading the layout store here, so this
   * component keeps its "a pane owns only what it is handed" contract -- and
   * so hiding the bar never hides the per-pane Source button, which lives in
   * `headerSlot` and is controlled separately by the caller. Defaults to
   * visible, which is what every existing caller already got.
   */
  showSourceBanner?: boolean
  /**
   * Accessible name for the pane region (tasks.md 4.2), e.g. "First chat".
   * Only meaningful once a second pane exists; without it the pane is not
   * announced as a landmark at all.
   */
  paneLabel?: string
}

export function ConversationPane({
  sessionId,
  isActive,
  headerAnchorRef,
  headerSlot,
  onOpenSource,
  showSourceBanner = true,
  paneLabel,
}: ConversationPaneProps) {
  const { session, messages, state, isLoading, isError, hasMissingEvents, hasUnsavedEvents } =
    useAgentSession(sessionId)
  const targetNav = useMessageTargetNav(session)
  const [flashId, setFlashId] = useState<string | null>(null)
  const transcriptRef = useRef<HTMLDivElement | null>(null)
  const setComposerDraft = useAgentDeskUiStore((s) => s.setDraft)
  const getComposerDraft = useAgentDeskUiStore((s) => s.getDraft)
  // Edit (tasks.md's message controls): puts the message's text back into
  // this session's composer draft for the user to revise and resend. There
  // is no backend command to amend a message already appended to a
  // session's transcript, so this is honestly a "resend" affordance, not an
  // in-place edit -- see `MessageActions`'s doc comment.
  const editMessage = (text: string) => {
    if (!sessionId) return
    // Put the message in the box AND take the person to it.
    //
    // This used to write the draft and stop: no focus, no scroll, no word. In
    // a long transcript the composer is off-screen, so the click looked like
    // it had done nothing -- while having quietly replaced whatever was
    // already typed there. The Revise button next door already solves both
    // halves; this is the same treatment.
    const previous = getComposerDraft(sessionId).trim()
    setComposerDraft(sessionId, text)
    const box = document.getElementById(`agent-desk-composer-${sessionId}`)
    if (box instanceof HTMLTextAreaElement) {
      box.focus()
      box.setSelectionRange(box.value.length, box.value.length)
    }
    toast(previous === '' ? 'Copied into the message box.' : 'Copied into the message box, replacing your draft.')
  }
  // Tracks whether the reader was near the bottom just before this render's
  // message list changed, so the auto-follow effect below can tell "a new
  // message arrived while I was reading the bottom" (follow it) apart from
  // "a new message arrived while I had scrolled up to reread something"
  // (leave the scroll position alone). See `src/lib/agentDeskScroll.ts`.
  const wasNearBottomRef = useRef(true)

  const userMessages = useMemo(() => userMessagesForRail(messages), [messages])

  // Thinking block (mockup's `.ag-thought`): fold each `thoughtSummary`
  // message into the reply that follows it, or render it standalone when it
  // has no reply yet -- see `foldThoughtSummaries`.
  const { thoughtFor, folded: foldedThoughtIds } = useMemo(() => foldThoughtSummaries(messages), [messages])

  // Event stack (mockup's `.ag-event-stack`): every run of consecutive
  // `tool`-kind messages becomes one compact activity feed rendered after
  // the message that preceded the run -- see `groupEventStacks`. `tool`
  // messages themselves are skipped from normal row rendering below.
  const eventGroups = useMemo(() => groupEventStacks(messages), [messages])
  const eventGroupsByAnchor = useMemo(() => {
    const map = new Map<string, EventStackGroup>()
    for (const group of eventGroups) map.set(group.afterMessageId, group)
    return map
  }, [eventGroups])

  const [railTicks, setRailTicks] = useState<ReturnType<typeof computeRailTicks>>([])
  // Each message's measured offset, kept so a scroll event can move the rail's
  // marker without touching the DOM again.
  const railInputsRef = useRef<Parameters<typeof computeRailTicks>[0]>([])
  // Measured transcript width, so the rail popup can be sized to at least
  // half of it (tasks.md 5.2) instead of a fixed rem value that has no
  // relationship to the pane it is jumping around in.
  const [transcriptWidth, setTranscriptWidth] = useState(0)

  // tasks.md 5.1: rail ticks are computed from each user message's real
  // offset within the transcript, not from its index -- a long tool-output
  // message between two short ones must not compress the rail's sense of
  // "how far apart these messages are". Re-measures after every layout pass
  // (new messages, content finishing streaming) and on window resize, since
  // a resize can reflow message heights and change every offset at once.
  useEffect(() => {
    const el = transcriptRef.current
    if (!el) return
    const measure = () => {
      const containerTop = el.getBoundingClientRect().top
      const inputs = userMessages.map((m) => {
        const node = el.querySelector<HTMLElement>(`[data-message-id="${m.messageId}"]`)
        const offsetTop = node ? node.getBoundingClientRect().top - containerTop + el.scrollTop : 0
        return { messageId: m.messageId, offsetTop }
      })
      // Remembered so the scroll handler can recompute the "you are here"
      // tick without re-measuring every message on every scroll event.
      railInputsRef.current = inputs
      setRailTicks(computeRailTicks(inputs, el.scrollHeight, currentMessageForScroll(inputs, el.scrollTop)))
      setTranscriptWidth(el.clientWidth)
    }
    measure()
    const observer = new ResizeObserver(measure)
    observer.observe(el)
    window.addEventListener('resize', measure)
    return () => {
      observer.disconnect()
      window.removeEventListener('resize', measure)
    }
  }, [userMessages])

  // tasks.md 4.5: auto-follow only when already near the bottom. Reads the
  // scroll position on every scroll event (not just before a new message
  // lands) so a manual scroll away is captured immediately, and re-checks
  // right after the message list grows so a reader at the bottom gets pulled
  // down to the newest content.
  useEffect(() => {
    const el = transcriptRef.current
    if (!el) return
    const onScroll = () => {
      wasNearBottomRef.current = isNearBottom({
        scrollTop: el.scrollTop,
        scrollHeight: el.scrollHeight,
        clientHeight: el.clientHeight,
      })
      // Move the rail's "you are here" tick. Recomputed from the remembered
      // offsets rather than re-measuring the DOM, so this stays cheap enough
      // to run on a passive scroll listener.
      const inputs = railInputsRef.current
      if (inputs.length > 0) {
        setRailTicks(computeRailTicks(inputs, el.scrollHeight, currentMessageForScroll(inputs, el.scrollTop)))
      }
    }
    el.addEventListener('scroll', onScroll, { passive: true })
    return () => el.removeEventListener('scroll', onScroll)
  }, [sessionId])

  useEffect(() => {
    const el = transcriptRef.current
    if (!el || messages.length === 0) return
    if (wasNearBottomRef.current) {
      el.scrollTop = el.scrollHeight
    }
  }, [messages])

  const jumpToMessage = (messageId: string) => {
    const el = transcriptRef.current?.querySelector(`[data-message-id="${messageId}"]`)
    if (el instanceof HTMLElement) {
      const reducedMotion = window.matchMedia?.('(prefers-reduced-motion: reduce)').matches
      el.scrollIntoView({ behavior: reducedMotion ? 'auto' : 'smooth', block: 'center' })
      el.focus({ preventScroll: true })
      setFlashId(messageId)
      window.setTimeout(() => setFlashId((current) => (current === messageId ? null : current)), 900)
    }
  }

  /**
   * The pane's own header, drawn identically in every state.
   *
   * tasks.md 4.4 ("show a loading skeleton inside the targeted pane without
   * clearing its header"): the loading, error, and no-selection branches
   * below used to return a bare centered message, which took the header --
   * and with it the per-pane Source/Context/Graph buttons in `headerSlot` --
   * off screen for as long as the chat took to load. That made a pane look
   * like it had lost its controls mid-click. Now only the *body* swaps.
   */
  const header = (
    <div ref={headerAnchorRef} className="flex flex-none items-center gap-2 border-b border-border px-3 py-1.5">
      <p className="min-w-0 flex-1 truncate text-xs font-semibold text-foreground">
        {session?.header.title || (session ? 'New chat' : sessionId ? 'Opening…' : 'No chat selected')}
      </p>
      {headerSlot}
    </div>
  )

  const shellClass = cn('flex h-full min-h-0 flex-1 flex-col', !isActive && 'opacity-90')
  /**
   * tasks.md 4.2: the pane needs an accessible active label, not just a
   * colour change. `aria-current="true"` is what a screen reader announces
   * for "this is the one your next click lands in"; the visible accent
   * border is drawn by the caller around the pane, since only the caller
   * knows whether there is a second pane to distinguish it from.
   */
  const shellProps = paneLabel
    ? ({ role: 'region', 'aria-label': paneLabel, 'aria-current': isActive } as const)
    : {}

  if (!sessionId) {
    return (
      <div className={shellClass} {...shellProps}>
        {header}
        <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-1.5 p-8 text-center">
          <p className="text-sm font-semibold text-foreground">No chat selected</p>
          <p className="max-w-xs text-xs leading-relaxed text-muted-foreground">
            Pick a chat on the left, or start a new one to begin.
          </p>
        </div>
      </div>
    )
  }

  if (isLoading) {
    return (
      <div className={shellClass} {...shellProps}>
        {header}
        {/* A shaped skeleton rather than a spinner: it stands where the
            source banner and first messages will land, so the pane does not
            visibly jump once the real content arrives. */}
        <div className="mx-auto flex min-h-0 w-full max-w-[52rem] flex-1 flex-col gap-3 p-3" aria-busy="true" aria-live="polite">
          <span className="sr-only">Opening this chat…</span>
          <div className="h-11 flex-none animate-pulse rounded-md bg-panel2 motion-reduce:animate-none" aria-hidden />
          <div className="flex flex-col gap-2" aria-hidden>
            {[0, 1, 2].map((row) => (
              <div key={row} className="flex gap-2.5">
                <span className="h-[27px] w-[27px] flex-none animate-pulse rounded-full bg-panel2 motion-reduce:animate-none" />
                <span
                  className="h-12 flex-1 animate-pulse rounded-md bg-panel2 motion-reduce:animate-none"
                  style={{ opacity: 1 - row * 0.25 }}
                />
              </div>
            ))}
          </div>
        </div>
      </div>
    )
  }

  if (isError || !session) {
    return (
      <div className={shellClass} {...shellProps}>
        {header}
        <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-1.5 p-8 text-center">
          <p className="text-sm font-semibold text-foreground">This chat could not be opened</p>
          <p className="max-w-xs text-xs leading-relaxed text-muted-foreground">
            Its saved file may be missing or damaged. Try another chat, or start a new one.
          </p>
        </div>
      </div>
    )
  }

  return (
    <div className={shellClass} {...shellProps}>
      {header}

      {/* tasks.md 8.2: hiding the source bars hides only this banner. The
          pane's own Source button (in `headerSlot`) is untouched, so the
          same information is always one click away.

          A `manual` chat is skipped. Every other kind names the thing the
          work came from -- an issue, a PR, a spec task -- which is the whole
          point of the row. `manual` has no such thing, so the banner read
          "Started without a specific issue, PR, or task" above every chat
          anyone had ever typed into: a permanent row stating an absence,
          taking height from the transcript on the chats that need it most.
          The new-chat screen still offers to link a source, which is where
          that sentence belongs -- there it is an invitation, here it was
          only a label. */}
      {showSourceBanner && session.header.source.kind !== 'manual' && (
        <SessionSourceBanner
          sessionId={sessionId}
          source={session.header.source}
          state={state ?? session.header.state}
          onOpenSource={onOpenSource}
        />
      )}

      {/* `flex-none` while the chat is empty, and only then. The composer
          takes `flex-1` in that state so the new-chat choices get the room
          (see `SessionComposer`), but this wrapper was claiming `flex-1`
          too -- so the two split the pane and the empty transcript held a
          tall blank band above the very form it was making space for. One
          of them has to yield, and it is the one with nothing in it. */}
      <div className={cn('relative flex min-h-0', messages.length === 0 ? 'flex-none' : 'flex-1')}>
        <div
          ref={transcriptRef}
          // Each row keeps a reading width in the middle of the pane rather than
          // stretching a line of chat across a maximised window.
          className="flex min-h-0 flex-1 flex-col gap-1 overflow-y-auto px-3 py-3 empty:p-0 *:mx-auto *:w-full *:max-w-[52rem]"
        >
          {messages.length === 0 ? (
            /* Nothing here on purpose: `SessionComposer` grows into this space
               with the mode, team and AI choices while a chat is empty. Two
               empty states stacked would have left the real one squeezed
               under a placeholder saying the same thing. */
            null
          ) : (
            messages.flatMap((m) => {
              // `tool` messages render as part of an `EventStack` (anchored
              // after the message preceding their run), never as their own
              // row -- see `groupEventStacks`.
              if (m.kind === 'tool') return []
              // A folded `thoughtSummary` renders inside the reply it
              // precedes (via `MessageRow`'s `thought` prop), so its own row
              // is skipped here -- see `foldThoughtSummaries`.
              if (foldedThoughtIds.has(m.messageId)) return []

              const nodes: React.ReactNode[] = []
              if (m.kind === 'thoughtSummary') {
                nodes.push(<StandaloneThoughtRow key={m.messageId} message={m} />)
              } else {
                nodes.push(
                  <MessageRow
                    key={m.messageId}
                    sessionId={sessionId}
                    message={m}
                    flash={flashId === m.messageId}
                    targetNav={targetNav}
                    onOpenSource={onOpenSource}
                    thought={thoughtFor.get(m.messageId)}
                    onEdit={m.role === 'user' ? editMessage : undefined}
                    answeredNote={m.kind === 'approval' ? gateAnswerNoteFor(messages, m) : null}
                  />
                )
              }

              const eventGroup = eventGroupsByAnchor.get(m.messageId)
              if (eventGroup) {
                nodes.push(
                  <EventStack
                    key={`${m.messageId}-events`}
                    items={eventGroup.items}
                    targetNav={targetNav}
                    onOpenSource={onOpenSource}
                  />
                )
              }
              return nodes
            })
          )}
          {/*
            Two warnings that look the same from a sequence number and need
            opposite advice.

            A missing event is one the file has and this window did not get:
            reopening the chat loads it. An unsaved event is the reverse -- the
            file is what is missing it, and the only copy is the one on screen,
            so reopening loads nothing and closing the app loses it. Telling
            someone to reopen in that case sends them to the emptier copy.

            The unsaved one is shown alone when both are true, because it is the
            one with something to lose and its advice contradicts the other's.
          */}
          {hasUnsavedEvents ? (
            <div
              role="status"
              className="flex items-start gap-1.5 rounded border border-[var(--gw-amber)]/40 bg-[var(--gw-amber)]/10 px-2 py-1 text-2xs leading-relaxed text-[var(--gw-amber)]"
            >
              <TriangleAlert size={12} className="mt-px flex-none" aria-hidden />
              <span>
                Part of this chat could not be saved. It is on screen now, but closing GitWyrm
                will lose it. Copy anything you want to keep.
              </span>
            </div>
          ) : hasMissingEvents ? (
            <div
              role="status"
              className="flex items-start gap-1.5 rounded border border-[var(--gw-amber)]/40 bg-[var(--gw-amber)]/10 px-2 py-1 text-2xs leading-relaxed text-[var(--gw-amber)]"
            >
              <TriangleAlert size={12} className="mt-px flex-none" aria-hidden />
              <span>Some of this chat did not reach this window. Close it and open it again to load the full record.</span>
            </div>
          ) : null}
          {runIsActive(state) && (
            // Announced: a screen-reader user needs the run state most, and it
            // was the one transition this pane changed silently. The predicate
            // is shared so this cannot drift out of step with the composer's
            // Stop button again -- it did, and `needsInput` fell through the
            // gap, leaving the transcript blank while an agent waited.
            <div role="status" aria-live="polite" className="flex items-center gap-2 px-1 py-1 text-2xs text-muted-foreground">
              <span
                className={cn(
                  'h-1.5 w-1.5 rounded-full',
                  // Waiting is not working: a pulse says "busy, sit tight",
                  // which is the wrong thing to say to someone whose answer
                  // is the only thing the run is missing.
                  state === 'needsInput'
                    ? 'bg-[var(--gw-amber)]'
                    : 'animate-pulse bg-primary motion-reduce:animate-none'
                )}
                aria-hidden
              />
              <span>{runActivityLabel(state)}</span>
            </div>
          )}
          {/* Startup recovery writes `failed`, while this line was gated only on
              `interrupted` -- so a chat recovered by the crash sweep showed no
              explanation at all. Two names for one situation.

              The gate was then hand-written a third time here while the
              sidebar and the graph used a wider one, and this copy dropped
              `missingSource`: a chat whose issue or spec could not be read
              showed a state dot in both of those panels and nothing at all in
              the conversation, which is the one place a person is reading.
              Both the test and the words now come from one place, because
              "stopped when the app closed" is simply wrong for that case and
              points at the wrong fix. */}
          {runStoppedBadly(state) && (
            <div role="status" aria-live="polite" className="flex items-center gap-2 px-1 py-1 text-2xs text-muted-foreground">
              <span className="h-1.5 w-1.5 rounded-full bg-muted-foreground" aria-hidden />
              <span>{runStoppedBadlyLabel(state)}</span>
            </div>
          )}
        </div>

        {/* Message rail (tasks.md 5.x): jump to any earlier message the user
            sent, mirroring the mockup's `.ag-history-rail` tick strip. Ticks
            are positioned from each message's real measured offset in the
            transcript (5.1, `railTicks`), not from its index. A hover/focus
            popup -- at least half the transcript's own width (5.2) -- lists
            every user message with its timestamp; the current one is
            highlighted, matching `.ag-history-jump.is-current`. Extracted to
            `MessageHistoryRail` -- see its doc comment for why the popup was
            anchoring at the bottom-left of the window instead of beside the
            rail, and how the hover/focus-driven open state works. */}
        <MessageHistoryRail
          userMessages={userMessages}
          railTicks={railTicks}
          transcriptWidth={transcriptWidth}
          onJumpToMessage={jumpToMessage}
        />
      </div>

      {/* R3.8: mount result review in the completed conversation.
          `ResultReviewPanel` existed since the review-and-landing package
          shipped but had zero importers anywhere in the app -- this is the
          one production entry point that was missing. Shown once this
          session's execution has actually stopped producing output
          (Finished/Failed/Stopped -- `shouldShowResultPanel`), keyed
          to `activeExecutionId` so it always reviews the run that just
          ended, not a stale earlier one. Read-only intents (Ask/Explain/
          Review/Summarize) still reach this: their result has no landable
          changes (`ResultRecord.hasLandableChanges` is false, since
          `worktreePath` is `None` for them -- policy.md's Never worktree
          policy), so `ResultReviewPanel` renders its changed-files/checks
          summary with no Keep/Commit actions rather than being hidden
          outright -- the user still gets to see what happened. */}
      {shouldShowResultPanel(state ?? session.header.state, session.header.activeExecutionId) && (
        <div className="flex-none border-t border-border empty:hidden">
          <ResultReviewPanel
            sessionId={sessionId}
            repoId={session.header.repoId}
            executionId={session.header.activeExecutionId!}
            intent={session.header.intent}
            taskText={session.header.title}
            // The AI that actually ran this execution, not a constant. A
            // hardcoded "copilot" put the wrong name in the drafted commit
            // for every Codex, Claude, Gemini and opencode result.
            provider={
              session.executions.find((e) => e.executionId === session.header.activeExecutionId)
                ?.provider ??
              session.header.preferredProvider ??
              'copilot'
            }
            // P1-C wiring 1: lets Keep also check off the originating
            // OpenSpec task -- see `ResultReviewPanel`'s `isOpenSpecTask`
            // doc comment.
            isOpenSpecTask={session.header.source.kind === 'openSpecTask'}
          />
        </div>
      )}

      {/* Composer (tasks.md 6.x): mode/team controls, draft, and Send --
          extracted to `SessionComposer` so this file's section-4/5 work
          (transcript, targets, auto-follow, history rail) is unaffected by
          composer changes and vice versa. */}
      <SessionComposer sessionId={sessionId} header={session?.header ?? null} isEmpty={messages.length === 0} />
    </div>
  )
}
