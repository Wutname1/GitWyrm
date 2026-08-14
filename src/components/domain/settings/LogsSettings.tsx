import { useState } from 'react'
import { Bug, FileText } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { LogsModal } from '@/components/modals/LogsModal'
import { ReportProblemModal } from '@/components/modals/ReportProblemModal'
import { ClearLogsButton, OpenLogsFolderButton, useClearLogs } from './LogActions'
import { SettingRow, SettingsGroup } from './SettingRow'

export function LogsSettings() {
  const [logsOpen, setLogsOpen] = useState(false)
  const [reportOpen, setReportOpen] = useState(false)
  const { clearing, clearLogs } = useClearLogs()

  return (
    <div>
      <SettingsGroup title="Get help">
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

        <SettingRow
          label="Application log"
          searchId="application-log"
          hint="View and search the diagnostic output GitWyrm writes each day. Logs older than two weeks are removed for you."
        >
          <div className="flex flex-wrap items-center gap-2">
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
      </SettingsGroup>

      <LogsModal open={logsOpen} onClose={() => setLogsOpen(false)} />
      <ReportProblemModal open={reportOpen} onClose={() => setReportOpen(false)} />
    </div>
  )
}
