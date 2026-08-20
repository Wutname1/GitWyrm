import { describe, expect, it } from 'vitest'
import {
  MODE_NOTES,
  canSendComposerDraft,
  modeToExecutionMode,
  teamToExecutionTeam,
} from './agentDeskComposer'

describe('canSendComposerDraft', () => {
  it('allows sending non-empty trimmed text with a session and nothing in flight', () => {
    expect(canSendComposerDraft({ draft: 'hello', sessionId: 's1', sending: false })).toBe(true)
  })

  it('blocks sending whitespace-only text', () => {
    expect(canSendComposerDraft({ draft: '   \n\t', sessionId: 's1', sending: false })).toBe(false)
  })

  it('blocks sending empty text', () => {
    expect(canSendComposerDraft({ draft: '', sessionId: 's1', sending: false })).toBe(false)
  })

  it('blocks a second send while one is already in flight (tasks.md 6.4)', () => {
    expect(canSendComposerDraft({ draft: 'hello', sessionId: 's1', sending: true })).toBe(false)
  })

  it('blocks sending without a selected session', () => {
    expect(canSendComposerDraft({ draft: 'hello', sessionId: null, sending: false })).toBe(false)
  })
})

describe('modeToExecutionMode', () => {
  it('maps every UI mode to its backend ExecutionMode', () => {
    expect(modeToExecutionMode('Ask')).toBe('ask')
    expect(modeToExecutionMode('Plan')).toBe('plan')
    expect(modeToExecutionMode('Auto')).toBe('auto')
  })
})

describe('teamToExecutionTeam', () => {
  it('maps solo and helpers to the backend ExecutionTeam values', () => {
    expect(teamToExecutionTeam('solo')).toBe('solo')
    expect(teamToExecutionTeam('helpers')).toBe('lead')
  })
})

describe('MODE_NOTES', () => {
  it('has a plain-language note for every mode', () => {
    expect(MODE_NOTES.Ask.length).toBeGreaterThan(0)
    expect(MODE_NOTES.Plan.length).toBeGreaterThan(0)
    expect(MODE_NOTES.Auto.length).toBeGreaterThan(0)
  })
})
