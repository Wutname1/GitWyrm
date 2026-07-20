import { useState } from 'react'
import { FileText } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { LogsModal } from '@/components/modals/LogsModal'
import { commands } from '@/lib/bindings'
import { ClearLogsButton, OpenLogsFolderButton, useClearLogs } from './LogActions'
import { SettingRow } from './SettingRow'

export function LogsSettings() {
  const [logsOpen, setLogsOpen] = useState(false)
  const { clearing, clearLogs } = useClearLogs()

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
          <OpenLogsFolderButton />
          <ClearLogsButton clearing={clearing} onClear={clearLogs} />
        </div>
      </SettingRow>

      <LogsModal open={logsOpen} onClose={() => setLogsOpen(false)} />
    </div>
  )
}
