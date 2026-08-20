import { describe, expect, it } from 'vitest'
import { resolveAgentDeskShellState, type AgentDeskShellInputs } from './agentDeskViewState'

const BASE: AgentDeskShellInputs = {
  hasRepoPath: true,
  repoError: null,
  repoReady: true,
  sessionsLoading: false,
  sessionCount: 0,
  hasSelection: false,
}

describe('resolveAgentDeskShellState', () => {
  it('is no-repository when the window has no repo path at all', () => {
    expect(resolveAgentDeskShellState({ ...BASE, hasRepoPath: false })).toBe('no-repository')
  })

  it('is load-failed when the repo path is known but opening it errored', () => {
    expect(
      resolveAgentDeskShellState({ ...BASE, repoReady: false, repoError: 'disk is unplugged' })
    ).toBe('load-failed')
  })

  it('no-repository takes priority over a stale load-failed error', () => {
    // Guards against a leftover error from a previous window mode leaking
    // through once the repo path itself has been cleared.
    expect(
      resolveAgentDeskShellState({ ...BASE, hasRepoPath: false, repoError: 'stale error' })
    ).toBe('no-repository')
  })

  it('is opening while the repo has not resolved yet and nothing has errored', () => {
    expect(resolveAgentDeskShellState({ ...BASE, repoReady: false })).toBe('opening')
  })

  it('is empty once the repo is ready, sessions finished loading, and there are none', () => {
    expect(resolveAgentDeskShellState({ ...BASE, sessionCount: 0 })).toBe('empty')
  })

  it('is not empty while sessions are still loading, even with a zero count so far', () => {
    expect(resolveAgentDeskShellState({ ...BASE, sessionsLoading: true, sessionCount: 0 })).toBe(
      'ready'
    )
  })

  it('is not empty once a session is selected, even before headers report a count', () => {
    expect(resolveAgentDeskShellState({ ...BASE, sessionCount: 0, hasSelection: true })).toBe(
      'ready'
    )
  })

  it('is ready once the repo is open and there is at least one session', () => {
    expect(resolveAgentDeskShellState({ ...BASE, sessionCount: 3 })).toBe('ready')
  })
})
