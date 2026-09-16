import { describe, expect, it } from 'vitest'
// @ts-expect-error -- no @types/node in this project; available at runtime
import { readFileSync, readdirSync } from 'node:fs'
// @ts-expect-error -- no @types/node in this project; available at runtime
import { fileURLToPath } from 'node:url'

/**
 * A failed read must never be rendered as a confident absence.
 *
 * This has now been found five times in five different components: a failed
 * settings scan reported as "you have no skills" (F69), a damaged session read
 * as "no usage data yet" (F90), an unreadable receipt list as "GitWyrm has not
 * changed any app's settings" (F100), an OpenSpec scan as "no active changes"
 * (F105), and a failed disk read as "no agent copies on disk" (F106).
 *
 * Each was fixed individually and the class kept recurring, so the check is
 * mechanical now. The rule: a component that renders `query.data ?? <empty>`
 * and then states an absence must also handle `isError`, or it is telling
 * someone that something is not there when the truth is that GitWyrm could not
 * look.
 *
 * Deliberately narrow. It flags only the `?? []`/`?? ''` shape on a `.data`
 * read, which is the exact shape all five defects had, rather than trying to
 * judge copy.
 */
/**
 * Every screen, not the two this defect was last found in.
 *
 * This watched `agent-desk` and `agent-setup` only, which is where the five
 * known instances happened to live. Running the same rule over the rest of
 * the app found three more the guard could never have seen: a branch menu
 * saying "No changes to pick from yet", a remotes dialog saying "No remotes
 * yet", and the branch manager saying "No branches yet" -- each while the
 * read was still running or had failed.
 *
 * A guard scoped to where a bug was last seen only ever catches that bug
 * again. Scanning everything needs no maintenance and has no blind spot; a
 * component with no query and no absence sentence simply matches nothing.
 */
function componentsInScope(): { name: string; source: string }[] {
  const out: { name: string; source: string }[] = []
  const walk = (dir: string, label: string) => {
    for (const entry of readdirSync(dir, { withFileTypes: true }) as {
      name: string
      isDirectory(): boolean
    }[]) {
      const full = `${dir}/${entry.name}`
      if (entry.isDirectory()) walk(full, `${label}${entry.name}/`)
      else if (entry.name.endsWith('.tsx')) {
        out.push({ name: `${label}${entry.name}`, source: readFileSync(full, 'utf8') })
      }
    }
  }
  walk(fileURLToPath(new URL('../components', import.meta.url)), '')
  return out
}

/**
 * Flagged for an absence sentence that belongs to a different query.
 *
 * Each was read before being listed. Shrinking this list is an improvement;
 * adding to it needs the same reading.
 */
const KNOWN_CROSS_TALK = new Set([
  // "No tasks yet" reads `change.tasks`, a prop that is already loaded.
  'domain/spec-desk/DeskDetail.tsx (history)',
  // `remotes` fills a column; the empty-state sentence is about branches,
  // which this component does guard.
  'modals/BranchManagerModal.tsx (remotes)',
  // "No groups match that search" is a different list, and `remoteMatches`
  // already has its own `isPending` handling.
  'modals/RepoPickerModal.tsx (remoteMatches)',
])

/**
 * Whether this file tells somebody there is nothing.
 *
 * Deliberately the shapes that assert emptiness about DATA -- "No remotes
 * yet", "no changes found", "none yet" -- and not every sentence containing
 * the word "no". A comment explaining that something is absent is not a
 * claim on a screen.
 */
function statesAnAbsence(source: string): boolean {
  return /(['"`>]\s*)(No|no)\s+[a-z]+(\s+[a-z]+)*\s*(yet|found|to pick from|match)/.test(source)
}

describe('a failed read is never shown as an absence', () => {
  it('every component that defaults query data to empty also handles a failure', () => {
    const offenders: string[] = []
    for (const { name, source } of componentsInScope()) {
      // `<name>.data ?? []` or `?? ''` -- a query result standing in for
      // "nothing", which is what every instance of this defect looked like.
      //
      // Checked per QUERY, not per file. The first version asked only whether
      // `isError` appeared anywhere in the source, so a file handling one
      // query's failure was cleared for swallowing a different query's --
      // which is exactly what `AgentSetupView` was doing: the inventory query
      // guarded, the detections query not, one `isError` covering both.
      for (const m of source.matchAll(/(\w+)\.data\s*\?\?\s*(?:\[\]|'')/g)) {
        const query = m[1]
        // BOTH states, not just failure. The delete-confirm dialog handled
        // `isError` and passed this guard while still showing a confident
        // "no working copy" during the whole window before its query
        // returned -- the query starts when the dialog opens, so `data` is
        // undefined, the list is empty and `isError` is false, all at once.
        //
        // "GitWyrm has not looked yet" and "GitWyrm could not look" are
        // different sentences but the same fact: it does not know. A guard
        // written for only the second half let the first half through.
        // Only a component that STATES an absence can state a false one.
        //
        // Widening the scan from two folders to the whole app surfaced two
        // dozen `?? []` defaults that feed a list and say nothing: an empty
        // sidebar section renders no rows, which is not a claim about
        // anything. Requiring a guard there would be defensive code for a
        // sentence nobody wrote.
        //
        // The defect is the SENTENCE -- "No remotes yet", "No changes to pick
        // from yet" -- shown when the honest answer is "still looking" or
        // "could not look".
        if (!statesAnAbsence(source)) continue
        // A file-wide check cannot bind a sentence to the query it describes.
        // Three components were flagged for a sentence about something else:
        // `DeskDetail`'s "No tasks yet" reads a prop, `BranchManagerModal`'s
        // remotes fill a column with no sentence, and `RepoPickerModal`'s
        // "No groups match" belongs to a different list and already handles
        // its own pending state.
        //
        // Named rather than excluded by rule: a cleverer regex would be
        // guessing, and a silent skip is how a guard starts passing over the
        // thing it was written for. This list is short, and each entry says
        // what was checked.
        if (KNOWN_CROSS_TALK.has(`${name} (${query})`)) continue
        const guarded =
          new RegExp(`${query}\.isError`).test(source) &&
          new RegExp(`${query}\.(isPending|isLoading)`).test(source)
        if (!guarded) offenders.push(`${name} (${query})`)
      }
    }
    expect(offenders, 'these treat a failed read as empty data').toEqual([])
  })
})

/**
 * The second shape of the same defect: a failure branch that states an
 * absence.
 *
 * The check above catches `query.data ?? []` followed by an empty state. It
 * does not catch the other way in, which an early-return component takes: an
 * explicit `if (query.isError) return <"nothing here">`. That branch KNOWS the
 * read failed and says "none" anyway, which is the more confident version of
 * the same lie, and the narrow pattern above passes it without comment
 * (verified by reverting the import surface's failure branch to "No chats
 * found for this AI tool." -- every existing check stayed green).
 *
 * Scoped to the phrasings that assert emptiness about the person's own work.
 * "Looking for…" and "GitWyrm could not…" are not absence claims and are meant
 * to appear near a failure branch.
 */
const ABSENCE_CLAIMS = [
  /\bno chats found\b/i,
  /\bno sessions found\b/i,
  /\bnothing (?:here|found|to show)\b/i,
  /\bhas no saved\b/i,
  /\bno .{0,24}\b(?:yet|available)\b/i,
]

/** The body of every `if (<something>.isError) { ... }` early return. */
function errorBranches(source: string): string[] {
  const out: string[] = []
  // Deliberately simple: matches the early-return shape this codebase uses,
  // `if (x.isError) {` ... up to the closing brace at the same indentation.
  for (const m of source.matchAll(/if\s*\([^)]*\.isError[^)]*\)\s*\{([\s\S]*?)\n\s{0,4}\}/g)) {
    if (m[1]) out.push(m[1])
  }
  // And the single-expression form, `if (x.isError) return <... />`.
  for (const m of source.matchAll(/if\s*\([^)]*\.isError[^)]*\)\s*return([^\n]*(?:\n[^\n]*){0,6})/g)) {
    if (m[1]) out.push(m[1])
  }
  return out
}

describe('a failure branch never states an absence', () => {
  it('no component answers a failed read with "there are none"', () => {
    const offenders: string[] = []
    for (const { name, source } of componentsInScope()) {
      for (const branch of errorBranches(source)) {
        const claim = ABSENCE_CLAIMS.find((re) => re.test(branch))
        if (claim) offenders.push(`${name} (matched ${claim})`)
      }
    }
    expect(
      offenders,
      'these tell someone nothing is there when the truth is GitWyrm could not look'
    ).toEqual([])
  })
})
