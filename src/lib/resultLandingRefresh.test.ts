import { describe, expect, it } from 'vitest'
import { QueryClient } from '@tanstack/react-query'
import { invalidateAfterResultLanding, keys } from './queryKeys'

/**
 * Keep/Undo/Commit on an agent result must refresh every surface that
 * shows the work: the result list alone was refreshed before, and the graph
 * panel, session row, task list and working-changes view stayed stale.
 */
describe('invalidateAfterResultLanding', () => {
  it('marks the session, OpenSpec and repository views stale', () => {
    const qc = new QueryClient()
    const repoId = 'repo-1'
    const sessionId = 'sess-1'
    const touched = [
      keys.agentResults(sessionId),
      keys.agentSession(sessionId),
      keys.agentSessionOpenspecStatus(sessionId),
      keys.agentSessionOpenspecContext(sessionId),
      keys.status(repoId),
      keys.repoCounts(repoId),
      keys.log(repoId),
      keys.worktrees(repoId),
      keys.openspecStatus(repoId),
      keys.openspecChanges(repoId),
    ] as const
    const untouched = [keys.status('other-repo'), keys.agentSession('other-session')] as const
    for (const key of [...touched, ...untouched]) qc.setQueryData(key, { seeded: true })

    invalidateAfterResultLanding(qc, repoId, sessionId)

    for (const key of touched) {
      expect(qc.getQueryState(key)?.isInvalidated, JSON.stringify(key)).toBe(true)
    }
    for (const key of untouched) {
      expect(qc.getQueryState(key)?.isInvalidated, JSON.stringify(key)).toBe(false)
    }
    // A diff under this repo is matched by prefix, however many segments it has.
    qc.setQueryData(['diff', repoId, 'src/a.rs', 'workingTree'], { seeded: true })
    invalidateAfterResultLanding(qc, repoId, sessionId)
    expect(qc.getQueryState(['diff', repoId, 'src/a.rs', 'workingTree'])?.isInvalidated).toBe(true)
  })
})
