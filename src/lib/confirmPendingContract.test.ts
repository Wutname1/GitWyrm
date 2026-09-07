import { describe, expect, it } from 'vitest'
// @ts-expect-error -- no @types/node in this project; available at runtime
import { readdirSync, readFileSync, statSync } from 'node:fs'
// @ts-expect-error -- no @types/node in this project; available at runtime
import { fileURLToPath } from 'node:url'

/**
 * `ConfirmDialog` closes itself the moment it is confirmed, unless the caller
 * passes `keepOpenOnConfirm`.
 *
 * So a `pending` prop without `keepOpenOnConfirm` is dead: the dialog is gone
 * before the value it would show ever becomes true. The button is never
 * disabled, the "Working…" label is never seen, and a second click lands on a
 * dialog that has not closed yet -- sending a destructive action twice.
 *
 * That is not hypothetical. Throwing away an agent's work passed
 * `pending={busy}` and `pendingLabel="Throwing away…"` and neither could ever
 * render; deleting a chat had no gate at all, and its second delete came back
 * "failed" because the chat was already gone, showing a red error for an
 * action that had worked.
 *
 * Someone adding `pending` is reaching for exactly the protection this
 * catches the absence of, so the pairing is worth enforcing rather than
 * remembering.
 */

const root = fileURLToPath(new URL('../', import.meta.url))

function sourceFiles(dir: string, out: string[] = []): string[] {
  for (const entry of readdirSync(dir)) {
    if (entry === 'node_modules') continue
    const full = `${dir}/${entry}`
    if (statSync(full).isDirectory()) sourceFiles(full, out)
    else if (entry.endsWith('.tsx') && !entry.includes('.test.')) out.push(full)
  }
  return out
}

/**
 * Each `<ConfirmDialog ... />` element in `source`, as its raw text.
 *
 * The naive version of this -- take everything from the opening tag to the
 * first `/>` -- is wrong here, and quietly so. These dialogs carry rich JSX
 * `description` props holding fragments and `{/* ... *\/}` comments, and both
 * contain `/>` and `*\/` sequences of their own. Cutting at the first one
 * ended the element before the props that matter, so two genuinely broken
 * dialogs read as clean.
 *
 * So instead: walk from the tag name counting `{`/`}` depth, and accept a
 * closing `/>` only at depth zero, where the element's own attribute list
 * lives. Strings inside braces are not tracked, because a lone brace inside a
 * string literal in a prop would be unusual enough to look at by hand.
 */
function confirmDialogElements(source: string): string[] {
  const out: string[] = []
  let from = 0
  for (;;) {
    const start = source.indexOf('<ConfirmDialog', from)
    if (start === -1) return out

    let depth = 0
    let i = start + '<ConfirmDialog'.length
    for (; i < source.length; i += 1) {
      const ch = source[i]
      if (ch === '{') depth += 1
      else if (ch === '}') depth -= 1
      else if (depth === 0 && ch === '/' && source[i + 1] === '>') {
        i += 2
        break
      }
    }

    out.push(source.slice(start, i))
    from = i
  }
}

describe('a confirm dialog that shows progress must stay open to show it', () => {
  it('never passes pending without keepOpenOnConfirm', () => {
    const offenders: string[] = []
    for (const file of sourceFiles(root)) {
      const source = readFileSync(file, 'utf8')
      for (const element of confirmDialogElements(source)) {
        const hasPending = /\bpending=/.test(element)
        const staysOpen = /\bkeepOpenOnConfirm\b/.test(element)
        if (hasPending && !staysOpen) {
          offenders.push(file.slice(root.length))
        }
      }
    }

    expect(
      offenders,
      offenders.length === 0
        ? ''
        : `These dialogs pass \`pending\` but close on confirm, so the pending ` +
            `state can never render and a second click can fire the action twice. ` +
            `Add \`keepOpenOnConfirm\` and close from the mutation's own callback:\n` +
            offenders.join('\n')
    ).toEqual([])
  })

  /** The check has to be able to see the shape it was written for. */
  it('recognises the broken pairing', () => {
    const broken = `<ConfirmDialog open={x} pending={busy} onConfirm={go} />`
    expect(confirmDialogElements(broken)).toHaveLength(1)
    expect(/\bpending=/.test(broken)).toBe(true)
    expect(/\bkeepOpenOnConfirm\b/.test(broken)).toBe(false)
  })

  it('accepts the fixed pairing', () => {
    const fixed = `<ConfirmDialog open={x} pending={busy} keepOpenOnConfirm onConfirm={go} />`
    const element = confirmDialogElements(fixed)[0]
    expect(/\bpending=/.test(element) && /\bkeepOpenOnConfirm\b/.test(element)).toBe(true)
  })
})
