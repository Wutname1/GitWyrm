import { Sparkles } from 'lucide-react'
import { toast } from 'sonner'
import { cn } from '@/lib/utils'
import { useSpecAi } from '@/hooks/useSpecAi'

/**
 * Which AI the Desk would use, in the titlebar.
 *
 * The single trust anchor for every AI action in this window: whatever the rail
 * offers, this says who would do it. It is deliberately always present -- a
 * missing chip would leave "is an AI involved here?" unanswered.
 *
 * Clicking it goes to Settings → AI. The Desk is a separate window with no
 * settings view of its own, so it says where to go rather than pretending to
 * navigate.
 */
export function AiProviderChip() {
  const ai = useSpecAi()

  const onClick = () => {
    if (ai.state === 'reconnect') {
      toast.warning(`Your ${ai.providerShort} sign-in needs renewing.`, {
        description:
          'Open Settings → AI in the main GitWyrm window and connect it again. Copying handoffs works either way.',
      })
      return
    }
    toast.info('Choose an AI in Settings → AI.', {
      description: 'It lives in the main GitWyrm window, under the gear icon.',
    })
  }

  const label =
    ai.state === 'ready'
      ? `${ai.provider}${ai.model ? ` · ${ai.model}` : ''}`
      : ai.state === 'reconnect'
        ? `${ai.providerShort} · reconnect`
        : 'AI · not set up'

  return (
    <button
      type="button"
      onClick={onClick}
      title={
        ai.state === 'ready'
          ? `Runs use ${ai.provider}. Click to change it.`
          : ai.state === 'reconnect'
            ? `${ai.providerShort} is signed in but has no models available. Click for what to do.`
            : 'No AI is set up. Click to see where to add one.'
      }
      className={cn(
        'titlebar-no-drag flex h-6 flex-none items-center gap-1.5 rounded-full border px-2.5 text-2xs font-semibold transition-colors',
        ai.state === 'ready' && 'border-primary/35 bg-soft text-accent-text hover:border-primary',
        ai.state === 'reconnect' &&
          'border-[var(--gw-amber)]/45 bg-[var(--gw-amber)]/10 text-[var(--gw-amber)] hover:border-[var(--gw-amber)]',
        ai.state === 'none' && 'border-border text-muted-foreground hover:text-sub'
      )}
    >
      <span
        aria-hidden
        className={cn(
          'size-1.5 flex-none rounded-full',
          ai.state === 'ready' && 'bg-primary',
          ai.state === 'reconnect' && 'bg-[var(--gw-amber)]',
          ai.state === 'none' && 'bg-muted-foreground'
        )}
      />
      {ai.state === 'ready' && <Sparkles size={10} strokeWidth={2.4} className="flex-none" />}
      <span className="max-w-[220px] overflow-hidden text-ellipsis whitespace-nowrap">
        {label}
      </span>
    </button>
  )
}
