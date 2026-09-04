import { describe, expect, it } from 'vitest'
// @ts-expect-error -- no @types/node in this project; available at runtime
import { readdirSync, readFileSync, statSync } from 'node:fs'
// @ts-expect-error -- no @types/node in this project; available at runtime
import { fileURLToPath } from 'node:url'

/**
 * React requires every hook to run on every render. A hook placed BELOW a
 * component-level `return` runs only on the renders that get past it, so the
 * hook count changes between renders and React throws "Rendered more hooks
 * than during the previous render".
 *
 * This is normally caught by ESLint's `react-hooks/rules-of-hooks`. This
 * project has no ESLint config, and `tsc` cannot see it, so nothing was
 * checking. The real instance -- two `useWorkspaceStore` calls added beneath
 * the loading and error returns in `AgentCatalog` -- crashed the entire window
 * to the crash-report screen on every ordinary open of the Agent Setup
 * providers tab, and shipped.
 *
 * The check is deliberately crude: within one exported component, does a line
 * calling a hook appear after a line that is a top-level `return`? It reports
 * where React would actually throw, and nothing else.
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

/** `[componentName, lineNumber, offendingLine]` for each violation found. */
function hooksAfterEarlyReturn(source: string): Array<[string, number, string]> {
  const lines = source.split('\n')
  const bad: Array<[string, number, string]> = []
  let current: string | null = null
  let sawReturn = false
  // Brace depth relative to the component body. Indentation is NOT reliable
  // here: a `return` inside a helper arrow function is indented the same as
  // one at the component's own level. The first version of this guard used
  // indentation and reported 53 components, every one of them false.
  let depth = 0
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i]
    if (current === null) {
      const decl = /^(?:export )?function ([A-Z]\w*)\s*\(/.exec(line)
      if (decl) {
        current = decl[1]
        sawReturn = false
        // The component's opening brace is on this very line, so count it
        // here. Skipping the line left every body statement at depth 0 and
        // the level gate below never matched -- the detector went silent,
        // which the self-check above is here to catch.
        depth = 0
        for (const ch of line) {
          if (ch === '{') depth++
          else if (ch === '}') depth--
        }
      }
      continue
    }
    const before = depth
    for (const ch of line) {
      if (ch === '{') depth++
      else if (ch === '}') depth--
    }
    if (depth <= 0 && /^\}/.test(line)) {
      current = null
      continue
    }
    // Only statements at the component's own body level count. `before === 1`
    // is the body itself; deeper is inside a callback, a block, or JSX.
    if (before === 1) {
      if (/^\s*return[ (;]/.test(line)) sawReturn = true
      // A guard `if (...) {` whose block returns also ends the render for
      // that path. Guards here are a handful of lines, so a short lookahead
      // is enough and avoids a full parse.
      if (/^\s*if\s*\(/.test(line) && /\breturn\b/.test(lines.slice(i, i + 6).join(' '))) sawReturn = true
      // A call inside JSX is an event handler body, not a render-time hook:
      // `onClick={() => void useSolo()}` names a local function that merely
      // starts with "use". Real hooks are bare statements or assignments at
      // the component's body level.
      // Specifically a JSX attribute (`onClick={...}`), not any arrow: real
      // hooks are routinely written `useWorkspaceStore((st) => st.x)`, and
      // excluding every `=>` blinded the detector to its own fixture.
      const isJsxCallback = /\w+=\{/.test(line)
      if (sawReturn && !isJsxCallback && /\buse[A-Z]\w*\(/.test(line) && !/^\s*(\/\/|\*)/.test(line)) {
        bad.push([current, i + 1, line.trim()])
      }
    }
  }
  return bad
}

describe('hook order contract', () => {
  it('catches the shape that actually shipped', () => {
    // The real AgentCatalog defect, reduced. If this stops failing, the
    // detector has gone blind and every result below is worthless.
    const broken = [
      'export function AgentCatalog() {',
      '  const query = useQuery({ queryKey: k })',
      '  if (query.isLoading) {',
      '    return <p>Looking…</p>',
      '  }',
      '  const tool = useWorkspaceStore((st) => st.defaultAgentTool)',
      '  return <div>{tool}</div>',
      '}',
    ].join('\n')
    const found = hooksAfterEarlyReturn(broken)
    expect(found).toHaveLength(1)
    expect(found[0][0]).toBe('AgentCatalog')
  })

  it('does not flag hooks that all precede the first return', () => {
    const fine = [
      'export function Fine() {',
      '  const a = useQuery({})',
      '  const b = useWorkspaceStore((st) => st.x)',
      '  if (!a) {',
      '    return null',
      '  }',
      '  return <div>{b}</div>',
      '}',
    ].join('\n')
    expect(hooksAfterEarlyReturn(fine)).toEqual([])
  })

  it('does not flag a hook inside a nested callback after a return', () => {
    const nested = [
      'export function Nested() {',
      '  const a = useQuery({})',
      '  if (!a) {',
      '    return null',
      '  }',
      '  return <List render={() => {',
      '      const inner = useMemoLike(1)',
      '      return inner',
      '    }} />',
      '}',
    ].join('\n')
    expect(hooksAfterEarlyReturn(nested)).toEqual([])
  })

  it('no component calls a hook after an early return', () => {
    const offenders: string[] = []
    for (const file of sourceFiles(`${root}components`)) {
      for (const [component, line, text] of hooksAfterEarlyReturn(readFileSync(file, 'utf8'))) {
        offenders.push(`${file.slice(root.length)}:${line} ${component} -- ${text}`)
      }
    }
    expect(offenders).toEqual([])
  })
})
