import { describe, expect, it } from 'vitest'
// Read from disk, not via `?raw`: vitest stubs CSS imports to an empty
// string, so a `?raw` CSS import silently asserts against nothing. Node
// built-ins are available (vitest.config.ts itself imports `node:path`);
// the `@ts-expect-error` covers this project not carrying `@types/node`.
// @ts-expect-error -- no @types/node in this project; available at runtime
import { readFileSync } from 'node:fs'
// @ts-expect-error -- no @types/node in this project; available at runtime
import { fileURLToPath } from 'node:url'

const css = readFileSync(fileURLToPath(new URL('../index.css', import.meta.url)), 'utf8')

/**
 * `bg-accent` does not do what its name suggests.
 *
 * `--accent` resolves to `--gw-panel3`, and `--gw-panel3` and `--gw-border`
 * are the same hex. So `border-accent bg-accent` renders a grey button with an
 * invisible edge -- which is what the approval gate's "Allow once" and the
 * result panel's Keep/Commit buttons looked like, on the two screens where a
 * primary action matters most. The real accent is `--primary`, which 48 other
 * domain components already use.
 *
 * These tests exist because that was found by reading a hex value, not by
 * anything failing. Nothing in the build objected.
 */
describe('accent token contract', () => {
  it('records that --accent is a grey surface, not the brand accent', () => {
    // If this ever stops being true, the guard below can be relaxed -- but it
    // should be a deliberate change, not a surprise.
    expect(css).toMatch(/--accent:\s*var\(--gw-panel3\)/)
    expect(css).toMatch(/--gw-panel3:\s*#2d2d2d/)
    expect(css).toMatch(/--gw-border:\s*#2d2d2d/)
  })

  it('only defines hover tokens that exist', () => {
    // `hover:bg-accent-hover` shipped on the Import button and did nothing,
    // because no such token is defined anywhere. Tailwind accepts any class
    // name, so neither the build nor the type checker objects.
    expect(css).not.toMatch(/--accent-hover/)
  })
})
