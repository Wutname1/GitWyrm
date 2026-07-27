import { useState } from 'react'
import { Bug, FileText } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { LogsModal } from '@/components/modals/LogsModal'
import { ReportProblemModal } from '@/components/modals/ReportProblemModal'
import { ClearLogsButton, OpenLogsFolderButton, useClearLogs } from './LogActions'
import { SettingRow } from './SettingRow'

export function LogsSettings() {
  const [logsOpen, setLogsOpen] = useState(false)
  const [reportOpen, setReportOpen] = useState(false)
  const { clearing, clearLogs } = useClearLogs()

  return (
    <div>
      <SettingRow
        label="Report a problem"
        searchId="report-problem"
        hint="Send us what went wrong. Your log is attached for you, with tokens and personal details removed."
      >
        <Button size="sm" className="h-7 gap-1.5 text-xs" onClick={() => setReportOpen(true)}>
          <Bug size={12} />
          Report a problem
        </Button>
      </SettingRow>

      <SettingRow label="Application log" searchId="application-log" hint="Diagnostic output written to the app log folder.">
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
      <ReportProblemModal open={reportOpen} onClose={() => setReportOpen(false)} />
    </div>
  )
}
