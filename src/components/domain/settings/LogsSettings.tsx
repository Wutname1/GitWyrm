import { useState } from 'react'
import { Eraser, FileText, FolderOpen } from 'lucide-react'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { LogsModal } from '@/components/modals/LogsModal'
import { commands } from '@/lib/bindings'
import { unwrap } from '@/lib/queryKeys'
import { SettingRow } from './SettingRow'

export function LogsSettings() {
  const [logsOpen, setLogsOpen] = useState(false)

  return (
    <div>
      <SettingRow label="Application log" hint="Diagnostic output written to the app log folder.">
        <div className="flex items-center gap-2">
          <Button
            variant="secondary"
            size="sm"
            className="h-7 gap-1.5 text-xs"
            onClick={() => setLogsOpen(true)}
          >
            <FileText size={12} />
            View logs
          </Button>
          <Button
            variant="secondary"
            size="sm"
            className="h-7 gap-1.5 text-xs"
            onClick={() => commands.openLogsFolder()}
          >
            <FolderOpen size={12} />
            Open folder
          </Button>
          <Button
            variant="secondary"
            size="sm"
            className="h-7 gap-1.5 text-xs"
            onClick={async () => {
              try {
                unwrap(await commands.clearLog())
                toast('Logs cleared')
              } catch (e) {
                toast.error(`Could not clear logs: ${(e as Error).message}`)
              }
            }}
          >
            <Eraser size={12} />
            Clear logs
          </Button>
        </div>
      </SettingRow>

      <LogsModal open={logsOpen} onClose={() => setLogsOpen(false)} />
    </div>
  )
}
