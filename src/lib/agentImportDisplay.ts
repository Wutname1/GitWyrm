import type { AdapterListEntry, ContinuationOutcome, ScannedExternalSession } from '@/lib/bindings'

/**
 * Pure display-copy logic for the external chat import UI
 * (`ImportPicker.tsx`), pulled out so it is directly testable in the
 * project's Node-env vitest setup (no jsdom/RTL) rather than only reachable
 * through a rendered component.
 */

/** What to show for one adapter row's detection state (task 4.1). A
 * disabled-but-detected adapter (OpenChamber today) must read differently
 * from "not found" -- the client exists, GitWyrm just does not support
 * reading it yet. */
export function detectionLabel(entry: AdapterListEntry): string {
  switch (entry.detection.kind) {
    case 'detected':
      if (!entry.enabled) return 'Found, not supported yet'
      return entry.detection.supported ? 'Found' : 'Found (older version)'
    case 'notDetected':
      return 'Not found on this computer'
    case 'failed':
      return 'Could not check'
  }
}

/** Whether an adapter row can be browsed at all: it must be both detected
 * and enabled. A detected-but-disabled adapter (or a supported=false
 * version) still shows in the list per task 4.1, but is not clickable. */
export function canBrowseAdapter(entry: AdapterListEntry): boolean {
  return entry.detection.kind === 'detected' && entry.enabled
}

export interface ProjectLabel {
  text: string
  /** `false` covers both "unresolved" and "no project recorded" -- both are
   * non-default states the row should visually flag, just with different
   * text. */
  resolved: boolean
  /** `true` only for a genuinely unresolved (as opposed to simply absent)
   * project path -- this is what should offer "link this folder," since
   * there is nothing to link for a session with no recorded path at all. */
  offerLinking: boolean
}

/** Project reconciliation copy for one scanned session row (task 2.4 /
 * 4.x's "honest unresolved-project UI"). */
export function projectLabel(session: ScannedExternalSession): ProjectLabel {
  switch (session.project.kind) {
    case 'resolved':
      return { text: session.project.repoName, resolved: true, offerLinking: false }
    case 'noProjectRecorded':
      return { text: 'No project recorded', resolved: false, offerLinking: false }
    case 'unresolved':
      return {
        text: `Project not found: ${session.project.recordedPath}`,
        resolved: false,
        offerLinking: true,
      }
  }
}

/** The label for the "Continue externally" action, or `null` when no
 * supported launch exists at all. Never returns "Continue session" for
 * anything less than a genuine resume -- see design.md: "never claims the
 * external client accepted context when it merely opened." */
export function continueExternallyLabel(outcome: ContinuationOutcome | undefined): string | null {
  if (!outcome) return null
  switch (outcome.kind) {
    case 'openOnly':
      return 'Open client'
    case 'unsupported':
    case 'clientNotDetected':
    case 'adapterDisabled':
      return null
  }
}
