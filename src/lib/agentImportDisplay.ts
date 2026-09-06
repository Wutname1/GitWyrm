import type {
  AdapterError,
  ImportScanOutcome,
  AdapterListEntry,
  ContinuationOutcome,
  ImportSessionOutcome,
  ScannedExternalSession,
} from '@/lib/bindings'

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

/**
 * What GitWyrm can say about continuing this chat in its original app, or
 * `null` when there is nothing to say.
 *
 * Never returns "Continue session" for anything less than a genuine resume --
 * see design.md: "never claims the external client accepted context when it
 * merely opened."
 *
 * This used to return the imperative "Open client", which the picker rendered
 * in a plain `<span>` beside an external-link icon: it read as a button, and
 * clicking it did nothing, because no launch command exists anywhere in the
 * app. The same principle that forbids overstating a resume forbids offering
 * an action GitWyrm cannot perform, so it now describes where the chat can be
 * continued rather than implying this app will take you there.
 */
export function continueExternallyLabel(outcome: ContinuationOutcome | undefined): string | null {
  if (!outcome) return null
  switch (outcome.kind) {
    case 'openOnly':
      return 'Can be continued in its own app'
    case 'unsupported':
    case 'clientNotDetected':
    case 'adapterDisabled':
      return null
  }
}

/** The GitWyrm session one picker row is currently linked to, or `null`.
 * A row is linked either because the scan already knew (`importedSessionId`
 * from the ledger) or because this row's own Import just succeeded and the
 * scan has not been refetched yet. After Unlink the caller resets the import
 * result and the scan drops the id, so this returns `null` again and every
 * linked-only action (Continue here, Open client, Unlink) disappears at
 * once (task 4.3). */
export function linkedImportedSessionId(
  session: ScannedExternalSession,
  imported: ImportSessionOutcome | undefined
): string | null {
  if (imported?.kind === 'created' || imported?.kind === 'refreshed') {
    return imported.session.header.sessionId
  }
  return session.importedSessionId ?? null
}

/** Plain-language copy for the Unlink confirmation. Kept here so the exact
 * promise it makes (messages stay, refresh stops, a later import makes a
 * new chat) is testable next to the backend behavior it describes. */
export function unlinkConfirmCopy(adapterName: string): { title: string; description: string } {
  return {
    title: `Unlink from ${adapterName}?`,
    description:
      `This chat stays in GitWyrm with every message it already has. It just stops being tied to ${adapterName}: ` +
      `Refresh will no longer pull in new messages from there, and the option to open ${adapterName} from this chat goes away. ` +
      `If you import the same ${adapterName} chat again later, it becomes a new chat instead of adding to this one.`,
  }
}

/**
 * The name a person would recognise for an adapter, from its stable id.
 *
 * The imported badge showed the raw id in its tooltip -- "Imported from
 * vscode-copilot" -- which is attribution to a slug rather than to a product.
 * The vision's rule is that imported output stays *visibly attributed* to the
 * client that produced it, and every adapter already carries a
 * `display_name` ("VS Code Copilot Chat", "Claude Code") that this side never
 * asked for.
 *
 * Falls back to the id rather than inventing a name: an adapter this build
 * does not know should say the unfamiliar thing it actually knows, not a
 * guess.
 */
export function adapterDisplayName(adapterId: string): string {
  switch (adapterId) {
    case 'claude-code':
      return 'Claude Code'
    case 'codex':
      return 'Codex'
    case 'opencode':
      return 'opencode'
    case 'vscode-copilot':
      return 'VS Code Copilot Chat'
    case 'openchamber':
      return 'OpenChamber'
    default:
      return adapterId
  }
}

/**
 * Plain-language result of importing a chat, and whether it worked.
 *
 * The failure path used to render the raw enum as its headline -- "Could not
 * import: corruptSession" -- while the same file's unlink handler ten lines
 * away already did this properly. A person meeting a damaged file learned
 * nothing about whose problem it was or what to do.
 *
 * The success path distinguishes a refresh that brought something from one
 * that brought nothing: `newMessageCount` was computed by the backend
 * specifically so the UI could say, and nothing read it, so twelve new
 * messages and none looked identical.
 */
export function explainImportOutcome(
  outcome: ImportSessionOutcome,
  title: string,
  adapterId: string
): { message: string; ok: boolean } {
  const client = adapterDisplayName(adapterId)
  switch (outcome.kind) {
    case 'created':
      return { message: `Imported "${title}"`, ok: true }
    case 'refreshed':
      return {
        message:
          outcome.newMessageCount === 0
            ? `"${title}" is already up to date`
            : `Added ${outcome.newMessageCount} new message${outcome.newMessageCount === 1 ? '' : 's'} to "${title}"`,
        ok: true,
      }
    case 'adapterDisabled':
      return { message: `GitWyrm cannot read ${client} chats yet.`, ok: false }
    case 'clientNotDetected':
      return { message: `${client} is not on this computer any more.`, ok: false }
    case 'sessionNotFound':
      return { message: `${client} no longer has that chat.`, ok: false }
    // Not a damaged file, and saying so would send someone hunting for a
    // fault that is not there. The other tool reused one id across two of
    // its own folders, which a restored backup or a synced profile can do.
    case 'ambiguousSession':
      return {
        message: `${client} has two saved chats with the same name for GitWyrm, so it cannot tell which one you meant.`,
        ok: false,
      }
    case 'corruptSession':
      return { message: `That chat's file could not be read: ${outcome.detail}`, ok: false }
    case 'writeFailed':
      return { message: `Could not save the imported chat: ${outcome.detail}`, ok: false }
  }
}

/**
 * Why a chat tool's sessions could not be listed.
 *
 * Nine outcomes collapsed into "No sessions available right now." -- which
 * reads as *you have no chats there*, a statement about the person's work,
 * when the truth is usually that GitWyrm could not look. `AdapterError` names
 * seven causes and carries the detail for each, and was referenced nowhere in
 * the app.
 *
 * `missingPath` is the likeliest of them in practice (five adapters produce
 * it) and the most fixable: the tool's folder moved.
 */
export function explainImportScanRefusal(
  outcome: Exclude<ImportScanOutcome, { kind: 'scanned' }>
): string {
  switch (outcome.kind) {
    case 'adapterDisabled':
      return 'Importing from this tool is turned off.'
    case 'clientNotDetected':
      return 'That tool does not appear to be installed on this computer.'
    case 'failed':
      return explainAdapterError(outcome.error)
  }
}

function explainAdapterError(error: AdapterError): string {
  switch (error.kind) {
    case 'clientNotDetected':
      return 'That tool does not appear to be installed on this computer.'
    case 'unsupportedVersion':
      return `That tool is version ${error.found}; GitWyrm can read ${error.supportedRange}.`
    case 'missingPath':
      return `GitWyrm looked in ${error.path} and it is not there. The tool may have moved its files.`
    case 'corruptSession':
      return `One of that tool's saved chats could not be read: ${error.detail}`
    case 'sessionNotFound':
      return 'That chat is no longer in the other tool.'
    case 'ambiguousSession':
      return 'That tool has two saved chats GitWyrm cannot tell apart, so it did not guess.'
    case 'timedOut':
      return 'That tool took too long to answer. It may be busy.'
    case 'io':
      return `GitWyrm could not read that tool's files: ${error.detail}`
  }
}

/**
 * Whether a chat's project is a stand-in rather than a real repository.
 *
 * Importing a session whose project folder GitWyrm cannot find still succeeds
 * -- the spec wants unresolved projects visible, not blocking -- so the chat
 * lands under a synthetic repo id `unresolved:<adapter>` with the literal
 * name "Unresolved project" (`commands/agent_import.rs`). The backend's own
 * comment says the UI is "expected to show the 'project not found' state
 * rather than a normal project-scoped row", and nothing did: the phrase was
 * printed in the same weight and colour as a real project name.
 *
 * Matching on the id, not the name, because the name is display text a future
 * change could reword while the id is the structural fact.
 */
export function isUnresolvedProject(repoId: string | null | undefined): boolean {
  return typeof repoId === 'string' && repoId.startsWith('unresolved:')
}
