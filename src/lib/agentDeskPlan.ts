/**
 * Parses a lead or reviewer message's plain-text body into plan checklist
 * rows, matching the mockup's `.ag-plan`/`.ag-plan-row` (a status icon, the
 * step text, and a right-aligned "Owner - status" in monospace).
 *
 * There is no dedicated `MessageKind` or wire type for a structured plan --
 * `src-tauri/src/agentdesk/bridge.rs::message_kind_for_step` maps
 * `RunStep::Plan` to plain `MessageKind::Assistant`, and its text is a
 * plain-language sentence, not a table. So this reads the lead's own
 * Markdown-style checklist convention out of `plainContent` (` - [ ] Step`,
 * `- [x] Step`, optionally followed by `(Owner - status)`), which is the
 * same convention GitHub/CommonMark checklists already use and the one the
 * lead's own plan-writing prompt is expected to produce. A message with no
 * checklist lines parses to an empty list, so callers can render nothing
 * rather than a plan with rows that do not exist.
 *
 * Pulled out of the component so the parsing is covered by a fast
 * `.test.ts` unit test (this project's `vitest.config.ts` runs
 * `src/**\/*.test.ts` in a Node environment with no DOM -- see
 * `src/lib/agentDeskRail.ts` for the same split).
 */

export type PlanRowState = 'done' | 'working' | 'pending'

export interface PlanRow {
  text: string
  state: PlanRowState
  /** Right-aligned label, e.g. "Luna - done" or "high confidence" -- absent when the line carries no parenthetical. */
  owner: string | null
}

const CHECKLIST_LINE = /^[-*]\s*\[([ xX~])\]\s*(.+)$/
const TRAILING_OWNER = /\(([^()]+)\)\s*$/

/** A checklist box's mark decides `done`/`working`/`pending`: `x` is done, `~` is actively working, blank is pending. */
function stateForMark(mark: string): PlanRowState {
  if (mark.toLowerCase() === 'x') return 'done'
  if (mark === '~') return 'working'
  return 'pending'
}

/**
 * Extracts plan rows from a message body. Each checklist line's trailing
 * `(...)` becomes the right-aligned owner/status label (the mockup's
 * `.ag-plan-owner`); a line with no parenthetical has no owner label and
 * still renders as a step (matching the review variant, which shows
 * "high confidence"/"looks good" instead of an owner name -- both are just
 * this same trailing-parenthetical convention).
 */
export function parsePlanChecklist(text: string): PlanRow[] {
  const rows: PlanRow[] = []
  // Lines inside a fenced block are an EXAMPLE of the convention, not a use
  // of it. Without this, an agent quoting the format it was asked to follow
  // -- or quoting a tasks file -- produced a real plan card with working
  // status icons, attached to a message that never claimed to report a plan.
  //
  // The OpenSpec parser reading the same syntax already tracks fences
  // (`src-tauri/src/openspec/parse.rs`). Same convention, two parsers, and
  // only one of them had thought about it.
  //
  // An UNCLOSED fence swallows the rest of the message, which is the safer
  // reading: a stray fence means the formatting went wrong, and a plan built
  // from the wreckage would be a confident answer to a question the text no
  // longer answers.
  let inFence = false
  for (const rawLine of text.split('\n')) {
    const line = rawLine.trim()
    if (line.startsWith('```') || line.startsWith('~~~')) {
      inFence = !inFence
      continue
    }
    if (inFence) continue
    const match = CHECKLIST_LINE.exec(line)
    if (!match) continue
    const [, mark, rest] = match
    const ownerMatch = TRAILING_OWNER.exec(rest)
    const stepText = ownerMatch ? rest.slice(0, ownerMatch.index).trim() : rest.trim()
    const owner = ownerMatch ? ownerMatch[1].trim() : null
    if (!stepText) continue
    rows.push({ text: stepText, state: stateForMark(mark), owner })
  }
  return rows
}
