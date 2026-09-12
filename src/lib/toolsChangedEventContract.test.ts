import { describe, expect, it } from 'vitest'
// @ts-expect-error -- no @types/node in this project; available at runtime
import { readFileSync } from 'node:fs'
// @ts-expect-error -- no @types/node in this project; available at runtime
import { fileURLToPath } from 'node:url'

/**
 * The two halves of an event name have to agree, and nothing makes them.
 *
 * The backend emits `agent-tools-changed` when a quiet re-check of the
 * installed AI tools finds something different from what the picker already
 * showed. The frontend listens for it and refetches. A typo on either side
 * fails silently in the worst way available: no error, no warning, and a
 * screen that simply keeps showing the old answer -- which is indistinguishable
 * from the feature working, because the old answer is usually right.
 *
 * Compared as strings read from both files, so this fails at `npm run
 * test:unit` rather than in somebody's running app.
 */
const root = fileURLToPath(new URL('../..', import.meta.url))

function rustConstant(): string {
  const source = readFileSync(`${root}/src-tauri/src/commands/agent_providers.rs`, 'utf8')
  const match = source.match(/pub const TOOLS_CHANGED_EVENT: &str = "([^"]+)"/)
  if (!match) throw new Error('the backend no longer declares TOOLS_CHANGED_EVENT')
  return match[1]
}

function typescriptConstant(): string {
  const source = readFileSync(`${root}/src/hooks/useAgentSessions.ts`, 'utf8')
  const match = source.match(/const TOOLS_CHANGED_EVENT = '([^']+)'/)
  if (!match) throw new Error('the frontend no longer declares TOOLS_CHANGED_EVENT')
  return match[1]
}

describe('the tools-changed event name', () => {
  it('is spelled the same on both sides', () => {
    expect(typescriptConstant()).toBe(rustConstant())
  })

  it('is still listened for where it is declared', () => {
    const source = readFileSync(`${root}/src/hooks/useAgentSessions.ts`, 'utf8')
    expect(source).toContain('listen(TOOLS_CHANGED_EVENT')
    // Every screen that lists tools shares this key prefix, so the
    // invalidation has to use the prefix rather than one screen's full key.
    expect(source).toContain("queryKey: ['agentProviders']")
  })
})
