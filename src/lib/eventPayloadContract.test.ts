import { describe, expect, it } from 'vitest'
// @ts-expect-error -- no @types/node in this project; available at runtime
import { readdirSync, readFileSync, statSync } from 'node:fs'
// @ts-expect-error -- no @types/node in this project; available at runtime
import { fileURLToPath } from 'node:url'

/**
 * Every shape that crosses between the Rust side and this one must be
 * described in exactly one place: `bindings.ts`, which is generated from the
 * Rust types themselves.
 *
 * Command parameters and returns get there automatically. **Event payloads do
 * not** -- the exporter only walks types reachable from a `#[tauri::command]`
 * signature, and a payload that only ever travels as an `emit` is reachable
 * from nothing. The fix is one line in `lib.rs`: `.typ::<ThePayload>()`.
 *
 * Two payloads never got that line, so the hooks receiving them hand-wrote
 * their own copy of the shape. That is the dangerous state: renaming a field
 * in Rust compiled, typechecked, and passed every test, while "View source"
 * and "View diff" silently stopped working, because the two sides had quietly
 * begun describing the same message differently and nothing compared them.
 *
 * This is the check that was missing. A `listen<T>(...)` whose payload type is
 * declared locally rather than imported from `bindings` is that same
 * unguarded state, so it fails here with the fix spelled out.
 *
 * **Scope.** Agent Desk only, and it is clean: all four of its listeners now
 * take the generated type. Pointing the same check at the rest of the app
 * finds six more (repo watching, updater progress, settings sync, AI commit
 * and conflict progress, the repo picker's git progress). Those are real
 * instances of the same risk -- `useRepoWatcher`'s payload even spells its
 * field `repo_id` with nothing checking it -- but each belongs to a surface
 * this change does not touch, and a check that fails on work nobody is doing
 * gets switched off rather than obeyed. Add a directory to `ROOTS` as each
 * one is cleared.
 */

/**
 * Directories this check covers. Every path is relative to `src/`.
 */
const ROOTS = [
  'components/domain/agent-desk',
  'components/domain/agent-setup',
  'hooks/useAgentDeskSourceListener.ts',
  'hooks/useAgentResultDiff.ts',
  'hooks/useAgentSessions.ts',
  'views/AgentDeskView.tsx',
]

const root = fileURLToPath(new URL('../', import.meta.url))

function sourceFiles(dir: string, out: string[] = []): string[] {
  for (const entry of readdirSync(dir)) {
    if (entry === 'node_modules') continue
    const full = `${dir}/${entry}`
    if (statSync(full).isDirectory()) sourceFiles(full, out)
    else if (/\.tsx?$/.test(entry) && !entry.includes('.test.')) out.push(full)
  }
  return out
}

/**
 * Payload type names used as `listen<Name>(`, paired with whether the file
 * declares `Name` itself rather than importing it.
 *
 * Deliberately narrow: only a named type argument is considered, because an
 * inline object literal (`listen<{ a: string }>`) is its own separate smell
 * and an anonymous one cannot be imported from `bindings` anyway.
 */
function locallyDeclaredListenPayloads(fullSource: string): string[] {
  // Import statements are removed before scanning. A multi-line import puts
  // each name on its own line as `  type Name,`, which reads exactly like a
  // declaration to a line-based check -- the first version of this reported
  // four listeners that import their payload correctly, which is the whole
  // failure mode it exists to prevent.
  const source = fullSource.replace(/import\s[\s\S]*?from\s*['"][^'"]*['"]/g, '')
  const bad: string[] = []
  const listens = source.matchAll(/listen<([A-Za-z_][\w]*)>\s*\(/g)
  for (const match of listens) {
    const name = match[1]
    // `interface Name {` or `type Name =` in this same file means the shape
    // is hand-written here rather than generated from the Rust struct.
    const declared = new RegExp(`(?:^|\\n)\\s*(?:export\\s+)?(?:interface|type)\\s+${name}\\b`).test(
      source
    )
    if (declared) bad.push(name)
  }
  return bad
}

describe('event payload types are generated, never hand-written', () => {
  it('has no listener declaring its own copy of a Rust payload shape', () => {
    const offenders: string[] = []
    const files = ROOTS.flatMap((r) => {
      const full = `${root}${r}`
      try {
        return statSync(full).isDirectory() ? sourceFiles(full) : [full]
      } catch {
        return []
      }
    })
    for (const file of files) {
      const source = readFileSync(file, 'utf8')
      for (const name of locallyDeclaredListenPayloads(source)) {
        offenders.push(`${file.slice(root.length)}: ${name}`)
      }
    }

    expect(
      offenders,
      offenders.length === 0
        ? ''
        : `These files hand-write a payload shape that Rust also defines, so the two can ` +
            `drift apart with nothing to catch it. Register the type in src-tauri/src/lib.rs ` +
            `with .typ::<ThePayload>(), regenerate bindings, and import it instead:\n` +
            offenders.join('\n')
    ).toEqual([])
  })

  /**
   * The check only earns its place if it can actually see the shape it was
   * written for, so it is run against the exact pattern that caused the bug.
   */
  it('recognises a hand-written payload interface', () => {
    const offending = `
      interface OpenResultDiffTarget { worktreePath: string }
      listen<OpenResultDiffTarget>(EVENT, () => {})
    `
    expect(locallyDeclaredListenPayloads(offending)).toEqual(['OpenResultDiffTarget'])
  })

  it('accepts a payload imported from the generated bindings', () => {
    const fixed = `
      import { type OpenResultDiffTarget } from '@/lib/bindings'
      listen<OpenResultDiffTarget>(EVENT, () => {})
    `
    expect(locallyDeclaredListenPayloads(fixed)).toEqual([])
  })
})
