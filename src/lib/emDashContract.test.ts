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
/**
 * Every screen, not two folders.
 *
 * This began as the two Agent Desk directories, which left four em dashes in
 * tooltips and labels a folder away -- a worktree row, a spec chip. The rule
 * in CLAUDE.md is about text people read, and text people read is not
 * confined to one feature.
 */
const COMPONENT_ROOT = '../components'

/**
 * Every module under `lib/`, rather than a chosen few.
 *
 * This started as four filenames, then became six name prefixes, and both
 * were the same mistake one step apart: a list somebody has to remember to
 * add to. The prefixes missed `tutorialLessons.ts` and `worktreeCopy.ts`,
 * which between them held seven em dashes in sentences people read -- the
 * exact defect this file exists to prevent, sitting outside its reach the
 * whole time.
 *
 * Scanning everything needs no maintenance and cannot develop a blind spot.
 * A module with no user-facing strings simply has nothing to match.
 */
function sourcesInScope(): { name: string; source: string }[] {
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
  walk(fileURLToPath(new URL(COMPONENT_ROOT, import.meta.url)), '')
  const libPath = fileURLToPath(new URL('.', import.meta.url))
  for (const name of readdirSync(libPath) as string[]) {
    if (!name.endsWith('.ts') || name.endsWith('.test.ts')) continue
    // `bindings.ts` is generated, and its contents are the backend's doc
    // comments rather than anything written for this screen.
    if (name === 'bindings.ts') continue
    out.push({ name, source: readFileSync(`${libPath}/${name}`, 'utf8') })
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
    // A dash standing alone, quoted, is the typographic "no value here" mark
    // -- what a version field shows before the build is known. The rule is
    // about a dash joining two clauses in a sentence, and that one joins
    // nothing.
    .filter(({ line }) => !/(['"`>]\s*)—(\s*['"`<])/.test(line))
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
