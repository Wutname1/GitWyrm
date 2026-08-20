import { useState } from 'react'
import { Bug, ExternalLink, FileText, Lightbulb } from 'lucide-react'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import type { TelemetryLevel } from '@/lib/bindings'
import { openWebUrl } from '@/lib/remoteWeb'
import { useWorkspaceStore } from '@/stores/workspaceStore'
import { LogsModal } from '@/components/modals/LogsModal'
import { ReportProblemModal } from '@/components/modals/ReportProblemModal'
import type { ReportKind } from '@/lib/feedback'
import { ClearLogsButton, OpenLogsFolderButton, useClearLogs } from './LogActions'
import { SettingRow, SettingsGroup } from './SettingRow'

/**
 * The reporting choices, in order of how much they send.
 *
 * The wording leads with what the reader gets, because that is the honest
 * reason to leave it on: a crash nobody reports is a crash nobody fixes, and a
 * platform with no install count is one whose bugs look like nobody's problem.
 * Linux made that concrete -- a package can be mirrored or repackaged, so
 * download numbers say nothing about whether anyone is running it.
 */
const TELEMETRY_CHOICES = [
  {
    value: 'off',
    label: 'Nothing',
    detail: 'GitWyrm sends no reports and is not counted.',
    toast: 'GitWyrm will send nothing',
  },
  {
    value: 'reports',
    label: 'Crashes and a daily count',
    detail:
      'Report errors so they can be fixed, and count this install once a day so your platform is known to have users.',
    toast: 'Crashes and the daily count will be sent',
  },
  {
    value: 'full',
    label: 'Crashes, count, and speed data',
    detail: 'Also send timings and logs, which is what makes slow operations findable.',
    toast: 'Crashes, the count, and speed data will be sent',
  },
] as const satisfies ReadonlyArray<{
  value: TelemetryLevel
  label: string
  detail: string
  toast: string
}>

export function LogsSettings() {
  const [logsOpen, setLogsOpen] = useState(false)
  const [reportOpen, setReportOpen] = useState(false)
  const [reportKind, setReportKind] = useState<ReportKind>('bug')
  const { clearing, clearLogs } = useClearLogs()
  // The effective value, not the stored one: an install that has never chosen
  // stores null, and the control must show the state it is actually in.
  const telemetryLevel = useWorkspaceStore((s) => s.telemetryLevelEffective)
  const setTelemetryLevel = useWorkspaceStore((s) => s.setTelemetryLevel)

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

      <SettingsGroup
        title="Privacy & data"
        blurb="Your choice takes effect the next time GitWyrm starts."
      >
        {/* Deliberately outside the "behavior" reset group (see
            SETTINGS_DEFAULTS): resetting this page must not widen what someone
            deliberately narrowed.

            One ordered choice rather than a switch per data type. Two
            independent switches made "on" ambiguous -- what each covered
            depended on the build's version and channel -- and the reader could
            not see what they were agreeing to without knowing that rule. */}
        <SettingRow
          label="What GitWyrm reports"
          searchId="telemetry-level"
          hint="Files, code, paths, and repository names are never sent, at any setting."
        >
          <div className="flex flex-col gap-2">
            {TELEMETRY_CHOICES.map((choice) => (
              <label
                key={choice.value}
                className="flex cursor-pointer items-start gap-2 text-xs text-foreground"
              >
                <input
                  type="radio"
                  name="telemetry-level"
                  value={choice.value}
                  checked={telemetryLevel === choice.value}
                  onChange={() => {
                    setTelemetryLevel(choice.value)
                    // The reporters are started once at launch, so choosing
                    // changes nothing visible until a restart. Without this the
                    // radio is the only feedback, which reads as "did that do
                    // anything?" (Rule #1).
                    toast.success(`${choice.toast} from your next start.`)
                  }}
                  className="mt-0.5 size-3.5 accent-[var(--gw-accent)]"
                />
                <span className="flex flex-col gap-0.5">
                  <span>{choice.label}</span>
                  <span className="text-2xs text-muted-foreground">{choice.detail}</span>
                </span>
              </label>
            ))}
            {/* The counts this feeds are public, so the page is one click away
                rather than an address to retype (Rule #1). */}
            <button
              type="button"
              onClick={() => openWebUrl('https://gitwyrm.com/stats', 'the stats page')}
              className="flex w-fit items-center gap-1.5 text-2xs text-accent-text hover:underline"
            >
              <ExternalLink size={11} />
              See the counts published at gitwyrm.com/stats
            </button>
          </div>
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
