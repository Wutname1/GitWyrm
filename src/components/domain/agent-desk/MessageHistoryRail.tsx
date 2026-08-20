import { useRef, useState } from 'react'
import type { SessionMessage } from '@/lib/bindings'
import { cn } from '@/lib/utils'
import { Popover, PopoverAnchor, PopoverContent } from '@/components/ui/popover'
import { truncateSnippet, type RailTick } from '@/lib/agentDeskRail'

/** How long the popup stays open after the pointer leaves the rail/popup, so crossing the gap between them does not flicker it shut. */
const CLOSE_DELAY_MS = 150

function formatClock(iso: string): string {
  const t = Date.parse(iso)
  if (Number.isNaN(t)) return ''
  return new Date(t).toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' })
}

/**
 * Jump-to-message tick rail (tasks.md 5.x), sitting beside the transcript's
 * right edge and opening a list of every user message on hover or focus.
 *
 * Split out of `ConversationPane` so its anchoring can be fixed in one place:
 * the rail button used to double as the Radix `PopoverTrigger` itself, which
 * is a `position: absolute` sliver with no intrinsic size of its own (it is
 * stretched by `top`/`bottom` insets). Mixing that into Radix's
 * click-to-open trigger machinery is what pinned the popup to the bottom of
 * the window instead of beside the rail -- Radix's floating layer measures
 * the *trigger* element via `getBoundingClientRect`, but this trigger's own
 * rect only exists at all if its `relative` ancestor has already been laid
 * out and sized, and toggling the popover from a click handler (rather than
 * before the anchor's own paint) meant the very first open of a given
 * conversation could measure the anchor as auto-height at the top of an
 * offscreen ancestor, before layout settled -- matching the reported
 * bottom-left pin.
 *
 * The fix: keep the rail's own wrapper (a normal, already-laid-out `<div>`
 * with `position: relative`, matching the mockup's `.ag-history-rail`) as
 * the single stable anchor, via `PopoverAnchor` -- the same pattern
 * `PaneDetailPopover` already uses for its header buttons. Open state is
 * driven by hover/focus below, not by Radix's own trigger click handling, so
 * there is no separate trigger element for the anchor rect to disagree with.
 */
export function MessageHistoryRail({
  userMessages,
  railTicks,
  transcriptWidth,
  onJumpToMessage,
}: {
  userMessages: SessionMessage[]
  railTicks: RailTick[]
  transcriptWidth: number
  onJumpToMessage: (messageId: string) => void
}) {
  const [open, setOpen] = useState(false)
  const closeTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)

  const clearCloseTimer = () => {
    if (closeTimerRef.current !== null) {
      clearTimeout(closeTimerRef.current)
      closeTimerRef.current = null
    }
  }

  const openNow = () => {
    clearCloseTimer()
    setOpen(true)
  }

  const scheduleClose = () => {
    clearCloseTimer()
    closeTimerRef.current = setTimeout(() => {
      closeTimerRef.current = null
      setOpen(false)
    }, CLOSE_DELAY_MS)
  }

  if (userMessages.length <= 1) return null

  return (
    <div
      className="absolute right-1 top-2 bottom-2 w-3.5"
      onMouseEnter={openNow}
      onMouseLeave={scheduleClose}
      onFocus={openNow}
      onBlur={(e) => {
        // Moving focus to the popup itself (a Radix portal, not a descendant
        // of this wrapper in the DOM) must not close it -- only leaving the
        // whole rail+popup pairing should. `relatedTarget` is the element
        // gaining focus, so check whether that lands inside the open popup.
        const next = e.relatedTarget
        if (next instanceof Node && next.closest('[data-slot="popover-content"]')) return
        scheduleClose()
      }}
    >
      <Popover open={open} onOpenChange={(next) => (next ? openNow() : scheduleClose())}>
        <PopoverAnchor asChild>
          <button
            type="button"
            aria-label="Jump to an earlier message"
            aria-expanded={open}
            onClick={() => setOpen((o) => !o)}
            onKeyDown={(e) => {
              if (e.key === 'Escape') scheduleClose()
            }}
            className="group absolute inset-0 outline-none"
          >
            {railTicks.map((tick) => (
              <span
                key={tick.messageId}
                style={{ top: `${tick.position * 100}%` }}
                className={cn(
                  'absolute right-0 block h-px w-1.5 -translate-y-1/2 rounded-full bg-muted-foreground/60 transition-colors group-hover:bg-accent-text motion-reduce:transition-none',
                  tick.isCurrent && 'h-0.5 w-3 bg-primary'
                )}
              />
            ))}
          </button>
        </PopoverAnchor>
        <PopoverContent
          side="left"
          align="start"
          sideOffset={3}
          onMouseEnter={openNow}
          onMouseLeave={scheduleClose}
          onOpenAutoFocus={(e) => e.preventDefault()}
          onEscapeKeyDown={() => setOpen(false)}
          style={{
            width: `min(${Math.max(transcriptWidth * 0.5, 288)}px, calc(100vw - 3rem))`,
          }}
          className="p-1 motion-reduce:transition-none motion-reduce:data-[state=open]:animate-none motion-reduce:data-[state=closed]:animate-none"
        >
          <div className="max-h-80 overflow-y-auto">
            {userMessages.map((m, i) => {
              const { text, truncated } = truncateSnippet(m.plainContent || 'message', 3)
              return (
                <button
                  key={m.messageId}
                  type="button"
                  onClick={() => {
                    onJumpToMessage(m.messageId)
                    setOpen(false)
                  }}
                  className={cn(
                    'block w-full rounded px-2 py-1.5 text-left text-2xs leading-snug text-sub hover:bg-panel2 hover:text-foreground',
                    i === userMessages.length - 1 && 'bg-soft text-foreground'
                  )}
                >
                  <span className="block whitespace-pre-wrap">
                    {text}
                    {truncated && '…'}
                  </span>
                  <time className="mt-0.5 block font-mono text-[10px] text-muted-foreground">
                    {formatClock(m.timestamp)} · you
                  </time>
                </button>
              )
            })}
          </div>
        </PopoverContent>
      </Popover>
    </div>
  )
}
