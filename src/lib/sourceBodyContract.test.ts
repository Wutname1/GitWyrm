import { describe, expect, it } from 'vitest'
// @ts-expect-error -- no @types/node in this project; available at runtime
import { readFileSync } from 'node:fs'
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
 * Deliberately a source scan rather than a render test. `LeftPanel` reaches
 * `window` through its import chain, so a test that only wants to know which
 * query a handler reads would bring a browser with it.
 */
const ROOT = fileURLToPath(new URL('..', import.meta.url))

function leftPanel(): string {
  return readFileSync(`${ROOT}/components/domain/left-panel/LeftPanel.tsx`, 'utf8')
}

describe('a chat started from the sidebar knows what it is about', () => {
  it('fetches the issue detail rather than sending the list row', () => {
    const source = leftPanel()
    const handler = source.slice(source.indexOf('const startIssueAiAction'), source.indexOf('const startPrAiAction'))
    expect(
      handler.includes('githubIssueDetail'),
      'the sidebar row is a summary with no body -- fetch the detail before starting, or the agent is told the labels and nothing else'
    ).toBe(true)
  })

  it('fetches the pull request detail rather than sending the list row', () => {
    const source = leftPanel()
    const start = source.indexOf('const startPrAiAction')
    const handler = source.slice(start, start + 2000)
    expect(
      handler.includes('githubPrDetail'),
      'same as the issue path: a review started from a summary describes the branches and never says what the pull request was for'
    ).toBe(true)
  })

  it('still starts a chat when the detail cannot be fetched', () => {
    const source = leftPanel()
    // Worse but not wrong: a chat that starts knowing less beats no chat.
    expect(source).toContain('detail ?? row')
  })
})
