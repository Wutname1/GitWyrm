import { describe, expect, it } from 'vitest'
// @ts-expect-error -- no @types/node in this project; available at runtime
import { readFileSync, readdirSync } from 'node:fs'
// @ts-expect-error -- no @types/node in this project; available at runtime
import { fileURLToPath } from 'node:url'

/**
 * Which ways of starting a chat a person can actually reach.
 *
 * The product describes the entry points as "an issue, a pull request, a
 * diff, a failed check, or an OpenSpec task". Two of those -- a diff and the
 * uncommitted working changes -- have a builder here, a shape the backend
 * stores with durable identity, and tests, and no menu item anywhere. A
 * failed check has the backend shape and no builder at all, so it is not in
 * either list below; this test would have to grow one when it gains one.
 *
 * That is the wiring-gap class this project has hit before -- a command that
 * existed, was registered, was tested, and was never called. It is recorded
 * here as a list rather than left to be rediscovered, and the test fails in
 * BOTH directions: wiring one up without moving it, or orphaning one that
 * works today.
 */
const ROOT = fileURLToPath(new URL('..', import.meta.url))

/** Reachable from a screen today. */
const WIRED = [
  'issueSourceInput',
  'pullRequestSourceInput',
  'commitSourceInput',
  'openSpecTaskSourceInput',
  'openSpecChangeSourceInput',
]

/** Built and stored, with nothing in the app that calls them. */
const NOT_WIRED = ['diffSourceInput', 'workingChangesSourceInput']

function componentSources(): string {
  const parts: string[] = []
  const walk = (dir: string) => {
    for (const name of readdirSync(dir, { withFileTypes: true }) as { name: string; isDirectory(): boolean }[]) {
      const full = `${dir}/${name.name}`
      if (name.isDirectory()) walk(full)
      else if (name.name.endsWith('.tsx')) parts.push(readFileSync(full, 'utf8'))
    }
  }
  walk(`${ROOT}/components`)
  return parts.join('\n')
}

describe('the ways a chat can be started', () => {
  const components = componentSources()

  it.each(WIRED)('%s is reachable from a screen', (fn) => {
    expect(
      components.includes(fn),
      `${fn} used to have a screen that called it and now does not. Either restore the entry point, or move it to NOT_WIRED and say why in its doc comment.`
    ).toBe(true)
  })

  it.each(NOT_WIRED)('%s is still built but unreachable', (fn) => {
    expect(
      components.includes(fn),
      `${fn} now HAS a caller, which is good -- move it to WIRED and delete the "no caller yet" note on it.`
    ).toBe(false)
  })

  it('counts every builder, so a new one cannot be left out of both lists', () => {
    const source = readFileSync(`${ROOT}/lib/agentDeskSources.ts`, 'utf8')
    const declared = [...source.matchAll(/export function (\w+SourceInput)\b/g)].map((m) => m[1])
    expect([...declared].sort()).toEqual([...WIRED, ...NOT_WIRED].sort())
  })
})
