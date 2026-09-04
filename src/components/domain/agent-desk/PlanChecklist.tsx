import { Circle, CircleCheck, LoaderCircle } from 'lucide-react'
import type { PlanRow } from '@/lib/agentDeskPlan'

/**
 * The structured plan checklist under a lead/reviewer message -- matching
 * the mockup's `.ag-plan`/`.ag-plan-row`: a status icon, the step text, and
 * a right-aligned "Owner - status" (or a review's "high confidence"/"looks
 * good") in monospace.
 *
 * Rows come from `parsePlanChecklist` (`src/lib/agentDeskPlan.ts`), which
 * reads the lead's own Markdown checklist convention out of a message's
 * plain text -- there is no dedicated wire type for a structured plan (see
 * that file's doc comment for why). This component only renders whatever
 * rows were parsed; a message with no checklist lines never mounts a
 * `PlanChecklist` at all (see `ConversationPane`).
 */
export interface PlanChecklistProps {
  rows: PlanRow[]
  /** aria-label for the list, matching the mockup's `aria-label="Lead agent plan"`. */
  label: string
}

function PlanStateIcon({ state }: { state: PlanRow['state'] }) {
  if (state === 'done') {
    return <CircleCheck size={14} className="text-[var(--gw-mint)]" aria-hidden />
  }
  if (state === 'working') {
    return (
      <LoaderCircle size={14} className="animate-spin text-[var(--gw-blue)] motion-reduce:animate-none" aria-hidden />
    )
  }
  return <Circle size={14} className="text-muted-foreground" aria-hidden />
}

const STATE_TEXT: Record<PlanRow['state'], string> = {
  done: 'Done',
  working: 'In progress',
  pending: 'Pending',
}

export function PlanChecklist({ rows, label }: PlanChecklistProps) {
  if (rows.length === 0) return null
  return (
    <div aria-label={label} className="mt-2 divide-y divide-[color-mix(in_srgb,var(--gw-border)_62%,transparent)] border-y border-border">
      {rows.map((row, i) => (
        <div key={`${row.text}-${i}`} className="grid min-h-[30px] grid-cols-[14px_minmax(0,1fr)_auto] items-center gap-2 py-1 text-2xs">
          <span className="sr-only">{STATE_TEXT[row.state]}</span>
          <PlanStateIcon state={row.state} />
          <span className="min-w-0 truncate text-foreground">{row.text}</span>
          {row.owner && (
            <span className="flex-none whitespace-nowrap font-mono text-[10px] text-muted-foreground">{row.owner}</span>
          )}
        </div>
      ))}
    </div>
  )
}
