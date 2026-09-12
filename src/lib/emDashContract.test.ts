import { describe, expect, it } from 'vitest'
// @ts-expect-error -- no @types/node in this project; available at runtime
import { readFileSync, readdirSync } from 'node:fs'
// @ts-expect-error -- no @types/node in this project; available at runtime
import { fileURLToPath } from 'node:url'

/**
 * No em dashes in anything a person reads.
 *
 * A house rule in CLAUDE.md, and one that keeps being broken quietly: an em
 * dash is what a model reaches for when joining two clauses, so it arrives in
 * new copy long after somebody last read the rule. Two had already landed on
 * screens people use -- a failing check rendered "npm run typecheck — 3
 * errors" on the panel where work is accepted, and the Agent setup row said
 * "Updating it is enough — GitWyrm found it".
 *
 * Checked mechanically because that is what this project does with a defect
 * class it has found more than once (see `absenceClaimContract.test.ts`).
 *
 * Deliberately narrow: only JSX text and string literals, not comments. The
 * rule covers comments too, but a comment is read by whoever is already in the
 * file, while this text is read by somebody who cannot see it coming.
 */
const DIRS = ['../components/domain/agent-desk', '../components/domain/agent-setup']
const LIB_FILES = ['agentDeskResult.ts', 'agentDeskComposer.ts', 'agentDeskGate.ts', 'aiModelList.ts']

function sourcesInScope(): { name: string; source: string }[] {
  const out: { name: string; source: string }[] = []
  for (const dir of DIRS) {
    const path = fileURLToPath(new URL(dir, import.meta.url))
    for (const name of readdirSync(path) as string[]) {
      if (!name.endsWith('.tsx')) continue
      out.push({ name, source: readFileSync(`${path}/${name}`, 'utf8') })
    }
  }
  const libPath = fileURLToPath(new URL('.', import.meta.url))
  for (const name of LIB_FILES) {
    try {
      out.push({ name, source: readFileSync(`${libPath}/${name}`, 'utf8') })
    } catch {
      // A file that has been renamed is not a failure of this rule.
    }
  }
  return out
}

/**
 * Lines with an em dash that are not comments.
 *
 * Line-based rather than a parse: a block comment's continuation lines start
 * with `*`, a line comment with `//`, and everything else on a line holding an
 * em dash is close enough to "text somebody reads" to be worth a human look.
 */
function offendingLines(source: string): string[] {
  return source
    .split('\n')
    .map((line, i) => ({ line: line.trim(), n: i + 1 }))
    .filter(({ line }) => line.includes('\u2014'))
    .filter(({ line }) => !line.startsWith('*') && !line.startsWith('//') && !line.startsWith('/*'))
    .map(({ line, n }) => `${n}: ${line.slice(0, 100)}`)
}

describe('no em dashes in text people read', () => {
  it('no agent-desk or agent-setup screen renders one', () => {
    const offenders: string[] = []
    for (const { name, source } of sourcesInScope()) {
      for (const line of offendingLines(source)) {
        offenders.push(`${name}:${line}`)
      }
    }
    expect(
      offenders,
      `em dashes in user-facing text (use a colon, a full stop, or rewrite):\n${offenders.join('\n')}`
    ).toEqual([])
  })

  it('finds one when there is one, so this cannot pass vacuously', () => {
    const planted = `const s = 'Updating it is enough \u2014 GitWyrm found it'`
    expect(offendingLines(planted)).toHaveLength(1)
    // And leaves prose in comments alone, which is the narrowing above.
    expect(offendingLines(`// a comment \u2014 with one`)).toEqual([])
    expect(offendingLines(` * a doc line \u2014 with one`)).toEqual([])
  })
})
