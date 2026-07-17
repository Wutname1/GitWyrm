import { Loader2 } from 'lucide-react'
import { cn } from '@/lib/utils'

export function PendingIndicator({ className }: { className?: string }) {
  return (
    <Loader2
      aria-hidden="true"
      className={cn('size-3.5 flex-none animate-spin text-current', className)}
      strokeWidth={2.2}
    />
  )
}
