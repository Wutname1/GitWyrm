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
      <SettingsGroup title="Report a problem">
        <SettingRow
          label="Report a problem"
          searchId="report-problem"
          hint="Create a report with useful app details. Tokens and personal details are removed."
        >
          <Button size="sm" className="h-7 gap-1.5 text-xs" onClick={() => setReportOpen(true)}>
            <Bug size={12} />
            Report a problem
          </Button>
        </SettingRow>

      </SettingsGroup>

      <SettingsGroup title="Diagnostic logs">
        <SettingRow
          label="Application log"
          searchId="application-log"
          hint="Read, search, open, or clear GitWyrm's recent activity log."
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
