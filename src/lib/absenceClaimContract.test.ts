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
const DIRS = ['../components/domain/agent-desk', '../components/domain/agent-setup']

function componentsInScope(): { name: string; source: string }[] {
  const out: { name: string; source: string }[] = []
  for (const dir of DIRS) {
    const path = fileURLToPath(new URL(dir, import.meta.url))
    for (const name of readdirSync(path) as string[]) {
      if (!name.endsWith('.tsx')) continue
      out.push({ name, source: readFileSync(`${path}/${name}`, 'utf8') })
    }
  }
  return out
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
