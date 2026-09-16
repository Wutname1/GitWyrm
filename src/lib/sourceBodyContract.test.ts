import { describe, expect, it } from 'vitest'
// @ts-expect-error -- no @types/node in this project; available at runtime
import { readFileSync, readdirSync } from 'node:fs'
// @ts-expect-error -- no @types/node in this project; available at runtime
import { fileURLToPath } from 'node:url'

/**
 * A chat started from an issue or a pull request is given its text.
 *
 * The two shapes differ by a field that is ABSENT rather than empty:
 * `IssueSummary` has no `body`, `IssueDetail` does, and the builder reads it
 * with `'body' in issue`. Hand it a row from the list query and the check
 * quietly answers false, so the agent received "[bug], assigned to ada" --
 * labels and nothing else -- while the same gesture from the issue panel sent
 * the actual problem. Nothing failed; the one thing the agent most needed was
 * the one thing missing.
 *
 * Checked at the CALLER, because the builder was never wrong: its own tests
 * already cover both shapes, including one named "with no body". What went
 * wrong was which shape a screen handed it.
 *
 * Every caller is FOUND, not named. This began as one hardcoded file and two
 * handler names, which was true on the day it was written and blind to any
 * third caller added after it -- the same narrow-scope fault three sibling
 * guards in this folder have already been widened out of.
 *
 * Deliberately a source scan rather than a render test. These components reach
 * `window` through their import chains, so a test that only wants to know
 * which query a handler reads would bring a browser with it.
 */
const ROOT = fileURLToPath(new URL('..', import.meta.url))

/** The builders that read a `body` the list row does not carry. */
const BUILDERS = [
  { call: 'issueSourceInput(', detail: 'githubIssueDetail', thing: 'issue' },
  { call: 'pullRequestSourceInput(', detail: 'githubPrDetail', thing: 'pull request' },
] as const

function componentsCalling(call: string): { name: string; source: string }[] {
  const out: { name: string; source: string }[] = []
  const walk = (dir: string, label: string) => {
    for (const entry of readdirSync(dir, { withFileTypes: true }) as {
      name: string
      isDirectory(): boolean
    }[]) {
      const full = `${dir}/${entry.name}`
      if (entry.isDirectory()) walk(full, `${label}${entry.name}/`)
      else if (entry.name.endsWith('.tsx')) {
        const source = readFileSync(full, 'utf8')
        // The import line names it too; a call has an open paren after it and
        // is not on an `import` line.
        const calls = source
          .split('\n')
          .some((line: string) => line.includes(call) && !line.trim().startsWith('import'))
        if (calls) out.push({ name: `${label}${entry.name}`, source })
      }
    }
  }
  walk(`${ROOT}/components`, '')
  return out
}

describe('a chat started from an issue or pull request knows what it is about', () => {
  for (const { call, detail, thing } of BUILDERS) {
    it(`every screen that starts one sends the ${thing}'s own text`, () => {
      const callers = componentsCalling(call)
      // A builder nobody calls is a different defect, caught by
      // `sourceEntryPointContract`. Here it would silently pass.
      expect(callers.length, `nothing calls ${call} any more`).toBeGreaterThan(0)

      const offenders = callers
        .filter(({ source }) => {
          // Either it fetches the full record itself, or it was handed one:
          // the panel reads a detail query directly (`issue.data`), which is
          // the same guarantee by a different route.
          const fetches = source.includes(detail)
          const readsADetailQuery = /\b(issue|pr)\.data\b/.test(source)
          return !fetches && !readsADetailQuery
        })
        .map(({ name }) => name)

      expect(
        offenders,
        `these hand a list row to the builder, and a row has no body: the agent is told the labels and nothing else. Fetch the ${thing} first, as LeftPanel does.`
      ).toEqual([])
    })
  }

  it('still starts a chat when the detail cannot be fetched', () => {
    // Worse but not wrong: a chat that starts knowing less beats no chat.
    const source = readFileSync(`${ROOT}/components/domain/left-panel/LeftPanel.tsx`, 'utf8')
    expect(source).toContain('detail ?? row')
  })
})
