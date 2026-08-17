import { useState } from 'react'
import { Bug, FileText, Lightbulb } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { LogsModal } from '@/components/modals/LogsModal'
import { ReportProblemModal } from '@/components/modals/ReportProblemModal'
import type { ReportKind } from '@/lib/feedback'
import { ClearLogsButton, OpenLogsFolderButton, useClearLogs } from './LogActions'
import { SettingRow, SettingsGroup } from './SettingRow'

export function LogsSettings() {
  const [logsOpen, setLogsOpen] = useState(false)
  const [reportOpen, setReportOpen] = useState(false)
  const [reportKind, setReportKind] = useState<ReportKind>('bug')
  const { clearing, clearLogs } = useClearLogs()

  const openReport = (kind: ReportKind) => {
    setReportKind(kind)
    setReportOpen(true)
  }

  return (
    <div>
      <SettingsGroup title="Get in touch">
        <SettingRow
          label="Report a problem"
          searchId="report-problem"
          hint="Create a report with useful app details. Tokens and personal details are removed."
        >
          <Button size="sm" className="h-7 gap-1.5 text-xs" onClick={() => openReport('bug')}>
            <Bug size={12} />
            Report a problem
          </Button>
        </SettingRow>

        <SettingRow
          label="Send feedback"
          searchId="send-feedback"
          hint="Share an idea or tell us what could work better. No log is sent unless you ask for it."
        >
          <Button
            variant="secondary"
            size="sm"
            className="h-7 gap-1.5 text-xs"
            onClick={() => openReport('feedback')}
          >
            <Lightbulb size={12} />
            Send feedback
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
      <ReportProblemModal
        open={reportOpen}
        initialKind={reportKind}
        onClose={() => setReportOpen(false)}
      />
    </div>
  )
}
