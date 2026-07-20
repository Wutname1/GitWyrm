import { useState } from 'react'
import { Eraser, FolderOpen } from 'lucide-react'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { PendingIndicator } from '@/components/ui/pending-indicator'
import { commands } from '@/lib/bindings'
import { unwrap } from '@/lib/queryKeys'

/**
 * Clearing the log, with its pending state. `onCleared` lets a caller that is
 * displaying the log drop what it has already read.
 */
export function useClearLogs(onCleared?: () => void) {
  const [clearing, setClearing] = useState(false)

  const clearLogs = async () => {
    setClearing(true)
    try {
      unwrap(await commands.clearLog())
      onCleared?.()
      toast('Logs cleared')
    } catch (e) {
      toast.error(`Could not clear logs: ${(e as Error).message}`)
    } finally {
      setClearing(false)
    }
  }

  return { clearing, clearLogs }
}

const BUTTON_CLASS = 'h-7 gap-1.5 text-xs'

export function OpenLogsFolderButton() {
  return (
    <Button
      variant="secondary"
      size="sm"
      className={BUTTON_CLASS}
      onClick={() => commands.openLogsFolder()}
    >
      <FolderOpen size={12} />
      Open folder
    </Button>
  )
}

export function ClearLogsButton({
  clearing,
  onClear,
}: {
  clearing: boolean
  onClear: () => void
}) {
  return (
    <Button
      variant="secondary"
      size="sm"
      className={BUTTON_CLASS}
      onClick={onClear}
      disabled={clearing}
      aria-busy={clearing || undefined}
    >
      {clearing ? <PendingIndicator /> : <Eraser size={12} />}
      {clearing ? 'Clearing…' : 'Clear logs'}
    </Button>
  )
}
