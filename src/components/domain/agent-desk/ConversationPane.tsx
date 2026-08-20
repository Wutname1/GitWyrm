import { useMemo, useRef, useState } from 'react'
import { toast } from 'sonner'
import { ArrowUp, ChevronUp, GitFork, Paperclip, Sparkles, User } from 'lucide-react'
import { commands, type SessionMessage } from '@/lib/bindings'
import { unwrap, keys } from '@/lib/queryKeys'
import { useQueryClient } from '@tanstack/react-query'
import { describeError, log } from '@/lib/log'
import { useAgentSession } from '@/hooks/useAgentSessions'
import { cn } from '@/lib/utils'
import { Markdown } from '@/components/ui/markdown'
import { Textarea } from '@/components/ui/textarea'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { SessionSourceBanner } from './SessionSourceBanner'

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

function formatClock(iso: string): string {
  const t = Date.parse(iso)
  if (Number.isNaN(t)) return ''
  return new Date(t).toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' })
}

/** Short avatar initials: "You" for the user, first two letters of the provider/model otherwise. */
function avatarInitials(message: SessionMessage): string {
  if (message.role === 'user') return 'You'
  const source = message.provider ?? 'Lead'
  return source.slice(0, 2)
}

function MessageRow({ message, flash }: { message: SessionMessage; flash: boolean }) {
  const isUser = message.role === 'user'
  return (
    <article
      id={`agent-desk-message-${message.messageId}`}
      data-message-id={message.messageId}
      tabIndex={-1}
      className={cn(
        'grid grid-cols-[27px_minmax(0,1fr)] gap-2.5 rounded-md px-2 py-1.5 outline-none transition-colors duration-500',
        flash && 'bg-[color-mix(in_srgb,var(--gw-accent)_18%,transparent)]'
      )}
    >
      <span
        className={cn(
          'flex h-[27px] w-[27px] flex-none items-center justify-center rounded-full text-[10px] font-bold uppercase',
          isUser ? 'bg-panel3 text-sub' : 'bg-soft text-accent-text'
        )}
        aria-hidden
      >
        {avatarInitials(message)}
      </span>
      <div className="min-w-0">
        <div className="mb-1 flex items-center gap-1.5 text-[11px] font-semibold text-foreground">
          <span>{kindLabel(message.kind)}</span>
          {message.provider && <span className="font-normal text-muted-foreground">{message.provider}</span>}
          <span className="font-normal text-muted-foreground">{formatClock(message.timestamp)}</span>
        </div>
        <div className="text-xs leading-relaxed text-foreground">
          {message.renderedContent ? (
            <Markdown text={message.renderedContent} />
          ) : (
            <p className="whitespace-pre-wrap">{message.plainContent}</p>
          )}
        </div>
      </div>
    </article>
  )
}

type ComposerMode = 'Ask' | 'Plan' | 'Auto'
type ComposerTeam = 'solo' | 'helpers'

const MODE_NOTES: Record<ComposerMode, string> = {
  Ask: 'Answers questions and reads the codebase; makes no changes',
  Plan: 'Drafts a plan and waits for you to start it',
  Auto: 'Lead may use up to 3 helpers and asks before risky actions',
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
}

export function ConversationPane({
  sessionId,
  isActive,
  headerAnchorRef,
  headerSlot,
}: ConversationPaneProps) {
  const { session, messages, state, isLoading, isError } = useAgentSession(sessionId)
  const qc = useQueryClient()
  const [draft, setDraft] = useState('')
  const [sending, setSending] = useState(false)
  const [flashId, setFlashId] = useState<string | null>(null)
  const [mode, setMode] = useState<ComposerMode>('Auto')
  const [team, setTeam] = useState<ComposerTeam>('helpers')
  const [teamOpen, setTeamOpen] = useState(false)
  const transcriptRef = useRef<HTMLDivElement | null>(null)

  const userMessages = useMemo(() => messages.filter((m) => m.role === 'user'), [messages])

  const jumpToMessage = (messageId: string) => {
    const el = transcriptRef.current?.querySelector(`[data-message-id="${messageId}"]`)
    if (el instanceof HTMLElement) {
      el.scrollIntoView({ behavior: 'smooth', block: 'center' })
      el.focus({ preventScroll: true })
      setFlashId(messageId)
      window.setTimeout(() => setFlashId((current) => (current === messageId ? null : current)), 900)
    }
  }

  const canSend = draft.trim().length > 0 && !sending && sessionId != null

  const send = async () => {
    if (!canSend || !sessionId) return
    setSending(true)
    const content = draft.trim()
    try {
      const outcome = unwrap(await commands.agentSessionAppendUserMessage(sessionId, content, []))
      if (outcome.kind === 'appended') {
        setDraft('')
        void qc.invalidateQueries({ queryKey: keys.agentSession(sessionId) })
        void qc.invalidateQueries({ queryKey: keys.agentSessionsAll })
      } else if (outcome.kind === 'notFound') {
        toast.error('This session is gone. It may have been archived elsewhere.')
      } else if (outcome.kind === 'damaged') {
        toast.error('This session file is damaged and could not accept the message.', {
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

  if (!sessionId) {
    return (
      <div className="flex h-full min-h-0 flex-1 flex-col items-center justify-center gap-1.5 p-8 text-center">
        <p className="text-sm font-semibold text-foreground">No chat selected</p>
        <p className="max-w-xs text-xs leading-relaxed text-muted-foreground">
          Pick a chat on the left, or start a new one to begin.
        </p>
      </div>
    )
  }

  if (isLoading) {
    return (
      <div className="flex h-full min-h-0 flex-1 flex-col items-center justify-center gap-1.5 p-8 text-center">
        <p className="text-sm font-semibold text-foreground">Opening this chat…</p>
        <p className="max-w-xs text-xs leading-relaxed text-muted-foreground">
          This takes a moment on first open.
        </p>
      </div>
    )
  }

  if (isError || !session) {
    return (
      <div className="flex h-full min-h-0 flex-1 flex-col items-center justify-center gap-1.5 p-8 text-center">
        <p className="text-sm font-semibold text-foreground">This chat could not be opened</p>
        <p className="max-w-xs text-xs leading-relaxed text-muted-foreground">
          Its saved file may be missing or damaged. Try another chat, or start a new one.
        </p>
      </div>
    )
  }

  return (
    <div className={cn('flex h-full min-h-0 flex-1 flex-col', !isActive && 'opacity-90')}>
      <div ref={headerAnchorRef} className="flex flex-none items-center gap-2 border-b border-border px-3 py-1.5">
        <p className="min-w-0 flex-1 truncate text-xs font-semibold text-foreground">
          {session.header.title || 'Untitled chat'}
        </p>
        {headerSlot}
      </div>

      <SessionSourceBanner source={session.header.source} state={state ?? session.header.state} />

      <div className="relative flex min-h-0 flex-1">
        <div ref={transcriptRef} className="flex min-h-0 flex-1 flex-col gap-1 overflow-y-auto px-3 py-3">
          {messages.length === 0 ? (
            <div className="flex flex-1 flex-col items-center justify-center gap-1 text-center">
              <p className="text-xs font-semibold text-foreground">No messages yet</p>
              <p className="max-w-xs text-2xs leading-relaxed text-muted-foreground">
                Say what you would like help with below.
              </p>
            </div>
          ) : (
            messages.map((m) => <MessageRow key={m.messageId} message={m} flash={flashId === m.messageId} />)
          )}
          {(state === 'working' || state === 'preparing') && (
            <div className="flex items-center gap-2 px-1 py-1 text-2xs text-muted-foreground">
              <span className="h-1.5 w-1.5 animate-pulse rounded-full bg-primary" aria-hidden />
              <span>{state === 'preparing' ? 'Getting ready…' : 'Working…'}</span>
            </div>
          )}
        </div>

        {/* Message rail (tasks.md 5.x): jump to any earlier message the user
            sent, mirroring the mockup's `.ag-history-rail` tick strip. A
            hover/focus popover lists every user message with its timestamp;
            the current one is highlighted, matching `.ag-history-jump.is-current`. */}
        {userMessages.length > 1 && (
          <Popover>
            <PopoverTrigger asChild>
              <button
                type="button"
                aria-label="Jump to an earlier message"
                className="group absolute right-1 top-2 bottom-2 flex w-3.5 flex-col items-end justify-center gap-1.5 outline-none"
              >
                {userMessages.map((m, i) => (
                  <span
                    key={m.messageId}
                    className={cn(
                      'block h-px rounded-full bg-muted-foreground/60 transition-colors group-hover:bg-accent-text',
                      i === userMessages.length - 1 ? 'h-0.5 w-3 bg-primary' : (i + 1) % 3 === 0 ? 'w-2.5' : 'w-1.5'
                    )}
                  />
                ))}
              </button>
            </PopoverTrigger>
            <PopoverContent side="left" align="end" className="w-[min(22rem,calc(100vw-3rem))] p-1">
              <div className="max-h-80 overflow-y-auto">
                {userMessages.map((m, i) => (
                  <button
                    key={m.messageId}
                    type="button"
                    onClick={() => jumpToMessage(m.messageId)}
                    className={cn(
                      'block w-full rounded px-2 py-1.5 text-left text-2xs leading-snug text-sub hover:bg-panel2 hover:text-foreground',
                      i === userMessages.length - 1 && 'bg-soft text-foreground'
                    )}
                  >
                    <span className="line-clamp-2">{m.plainContent || 'message'}</span>
                    <time className="mt-0.5 block font-mono text-[10px] text-muted-foreground">
                      {formatClock(m.timestamp)} · you
                    </time>
                  </button>
                ))}
              </div>
            </PopoverContent>
          </Popover>
        )}
      </div>

      <div className="flex-none border-t border-border p-2">
        <div className="rounded-lg border border-border bg-panel2 p-1.5">
          <div className="mb-1.5 flex flex-wrap items-center gap-1 px-0.5" role="group" aria-label="Agent operating mode">
            <span className="mr-0.5 text-2xs text-muted-foreground">Mode</span>
            {(['Ask', 'Plan', 'Auto'] as const).map((m) => (
              <button
                key={m}
                type="button"
                onClick={() => setMode(m)}
                aria-pressed={mode === m}
                className={cn(
                  'rounded px-1.5 py-0.5 text-2xs font-semibold',
                  mode === m ? 'bg-soft text-accent-text' : 'text-sub hover:bg-panel3 hover:text-foreground'
                )}
              >
                {m}
              </button>
            ))}
            <span className="ml-auto min-w-0 truncate text-2xs text-muted-foreground">{MODE_NOTES[mode]}</span>
          </div>

          <Textarea
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
            <button
              type="button"
              onClick={() => toast('Choose context to attach', { description: 'Coming soon.' })}
              aria-label="Attach context"
              className="flex h-6 w-6 flex-none items-center justify-center rounded text-muted-foreground hover:bg-panel3 hover:text-foreground"
            >
              <Paperclip size={13} />
            </button>
            <span className="flex flex-none items-center gap-1 text-2xs text-muted-foreground">
              <Sparkles size={12} className="text-accent-text" />
              <strong className="font-semibold text-foreground">Sol</strong> lead · 3 helpers max
            </span>

            <Popover open={teamOpen} onOpenChange={setTeamOpen}>
              <PopoverTrigger asChild>
                <button
                  type="button"
                  className="flex flex-none items-center gap-1 rounded px-1.5 py-0.5 text-2xs font-semibold text-sub hover:bg-panel3 hover:text-foreground"
                >
                  <GitFork size={12} />
                  {team === 'solo' ? 'Solo agent' : 'Lead + helpers'}
                  <ChevronUp size={11} />
                </button>
              </PopoverTrigger>
              <PopoverContent side="top" align="start" className="w-72 p-2">
                <div className="mb-2 flex items-center gap-2">
                  <GitFork size={13} className="text-accent-text" />
                  <strong className="text-xs font-semibold">Who works on this chat?</strong>
                  <span className="ml-auto text-[10px] text-muted-foreground">Change at any time</span>
                </div>
                <div className="flex flex-col gap-1.5">
                  <button
                    type="button"
                    onClick={() => {
                      setTeam('solo')
                      setTeamOpen(false)
                    }}
                    className={cn(
                      'flex items-start gap-2 rounded-md border border-border px-2 py-1.5 text-left',
                      team === 'solo' ? 'border-primary/50 bg-soft' : 'hover:bg-panel3'
                    )}
                  >
                    <User size={14} className="mt-0.5 flex-none text-muted-foreground" />
                    <span className="min-w-0 flex-1">
                      <strong className="block text-2xs font-semibold text-foreground">Solo agent</strong>
                      <span className="block text-[10.5px] leading-snug text-muted-foreground">
                        One agent owns the chat. No graph is created.
                      </span>
                    </span>
                    <span className="flex-none font-mono text-[9px] text-muted-foreground">1 agent</span>
                  </button>
                  <button
                    type="button"
                    onClick={() => {
                      setTeam('helpers')
                      setTeamOpen(false)
                    }}
                    className={cn(
                      'flex items-start gap-2 rounded-md border border-border px-2 py-1.5 text-left',
                      team === 'helpers' ? 'border-primary/50 bg-soft' : 'hover:bg-panel3'
                    )}
                  >
                    <GitFork size={14} className="mt-0.5 flex-none text-muted-foreground" />
                    <span className="min-w-0 flex-1">
                      <strong className="block text-2xs font-semibold text-foreground">Lead + helpers</strong>
                      <span className="block text-[10.5px] leading-snug text-muted-foreground">
                        Sol splits safe work into a graph. Plan lets you approve it first; Auto starts it when useful.
                      </span>
                    </span>
                    <span className="flex-none font-mono text-[9px] text-muted-foreground">up to 3</span>
                  </button>
                </div>
              </PopoverContent>
            </Popover>

            <span className="flex-1" />

            {/* Exactly one action button: Send. There is deliberately no
                separate stop/icon-only button beside it -- "Stop all" lives
                only in the Graph panel header (tasks.md 6.5). */}
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
    </div>
  )
}
