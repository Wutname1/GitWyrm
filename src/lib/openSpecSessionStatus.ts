import type { OpenSpecSessionStatus } from '@/lib/bindings'

export type OpenSpecStatusTone = 'neutral' | 'amber' | 'red'

export interface OpenSpecStatusLine {
  text: string
  tone: OpenSpecStatusTone
}

/**
 * Plain-language line for an OpenSpec session source's archived/moved/
 * deleted state (`agent-desk-openspec-workflows` tasks.md 4.5, section 7).
 *
 * `null` means "show nothing here" -- either everything is fine (`active`),
 * the question does not apply (`notAnOpenSpecSource`), or the status could
 * not be determined right now (session-level failures), in which case the
 * caller's own generic "no longer available" fallback already covers it.
 *
 * Pulled out of `SessionSourcePanel.tsx` so this mapping -- which state gets
 * which words and which severity -- is covered by a fast `.test.ts` unit
 * test; this project's `vitest.config.ts` runs `src/**\/*.test.ts` in a Node
 * environment with no DOM, so the component itself is not unit-testable
 * here (see `src/lib/agentDeskTargets.ts` for the same extraction pattern).
 */
export function openSpecStatusLine(status: OpenSpecSessionStatus | undefined): OpenSpecStatusLine | null {
  if (!status) return null
  switch (status.kind) {
    case 'active':
    case 'notAnOpenSpecSource':
      return null
    case 'archived':
      return { text: 'This change is finished and archived.', tone: 'neutral' }
    case 'moved':
      return {
        text: status.archived
          ? `Looks like it was renamed to "${status.likelyNewId}" and archived.`
          : `Looks like it was renamed to "${status.likelyNewId}".`,
        tone: 'amber',
      }
    case 'deleted':
      return { text: 'This change no longer exists in the repository.', tone: 'amber' }
    case 'repoNotOpen':
      return { text: 'Open this repository to see its current status.', tone: 'neutral' }
    case 'sessionNotFound':
    case 'sessionDamaged':
    case 'sessionUnavailable':
      return null
  }
}
