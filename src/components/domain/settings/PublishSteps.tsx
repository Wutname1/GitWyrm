import { Check, Copy, ExternalLink } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'

/**
 * The two steps that are still the user's to do after a signing key exists:
 * copy the public half, and add it on their host.
 *
 * Spelled out as numbered steps rather than a pair of buttons because until
 * both are done commits show as Unverified, which reads as signing being broken
 * rather than as setup being unfinished. Shared by the GPG and SSH cards so the
 * wording and the tick behaviour cannot drift apart.
 */
export function PublishSteps({
  copied,
  opened,
  onCopy,
  onOpen,
  openLabel = 'Paste it on GitHub and save',
}: {
  copied: boolean
  opened: boolean
  onCopy: () => void
  onOpen: () => void
  openLabel?: string
}) {
  return (
    <ol className="mt-2.5 grid gap-2">
      <Step
        done={copied}
        index={1}
        label="Copy the public key"
        action={
          <Button size="sm" variant="secondary" className="h-7" onClick={onCopy}>
            {copied ? <Check size={12} className="text-emerald-500" /> : <Copy size={12} />}
            {copied ? 'Copied' : 'Copy public key'}
          </Button>
        }
      />
      <Step
        done={opened}
        index={2}
        label={openLabel}
        action={
          <Button size="sm" variant="secondary" className="h-7" onClick={onOpen}>
            <ExternalLink size={12} />
            Open GitHub
          </Button>
        }
      />
    </ol>
  )
}

/** One numbered step, ticked once the user has done it. */
function Step({
  done,
  index,
  label,
  action,
}: {
  done: boolean
  index: number
  label: string
  action: React.ReactNode
}) {
  return (
    <li className="flex items-center gap-2.5">
      <span
        className={cn(
          'grid size-4 flex-none place-items-center rounded-full text-[9px] font-bold',
          done ? 'bg-emerald-500 text-white' : 'bg-border text-muted-foreground'
        )}
      >
        {done ? <Check size={10} strokeWidth={3} /> : index}
      </span>
      <span
        className={cn(
          'min-w-0 flex-1 truncate text-2xs',
          done ? 'text-muted-foreground line-through' : 'text-foreground'
        )}
      >
        {label}
      </span>
      {action}
    </li>
  )
}
