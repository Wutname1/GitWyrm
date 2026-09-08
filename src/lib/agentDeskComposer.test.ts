import { describe, expect, it } from 'vitest'
import {
  READ_ONLY_REASON,
  MODE_NOTES,
  TEAM_NEEDS_MODE_REASON,
  canSendComposerDraft,
  isModeBlocked,
  isTeamBlocked,
  modeToExecutionMode,
  teamToExecutionTeam } from './agentDeskComposer'

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

describe('READ_ONLY_REASON', () => {
  it('says why in plain words, without naming anything internal', () => {
    // The pills' tooltip and the note beside them share this one string so
    // they cannot drift apart.
    expect(READ_ONLY_REASON).toMatch(/cannot change files/)
    expect(READ_ONLY_REASON).not.toMatch(/intent|policy|worktree|canWrite/i)
  })
})

describe('busy flags belong to the chat that set them', () => {
  // The composer is NOT remounted when a pane switches chats -- the only key
  // in the pane tree is `key={pane}`. So every in-flight flag it holds
  // survives the swap, and sending in one chat then switching showed the
  // second chat a disabled button reading "Sending…" for work happening
  // somewhere else.
  //
  // `startFailure` already had this rule ("A failure card belongs to the chat
  // it happened in"); the four busy flags were left out of the same effect.
  // Checked at source level because the defect is about state lifetime across
  // a prop change, which the pure helpers above cannot express.
  const FLAGS = ['setSending(false)', 'setStopping(false)', 'setRetrying(false)', 'setChangingProject(false)']

  it('clears every in-flight flag when the pane changes chat', async () => {
    // @ts-expect-error -- no @types/node in this project; available at runtime
    const { readFileSync } = await import('node:fs')
    // @ts-expect-error -- no @types/node in this project; available at runtime
    const { fileURLToPath } = await import('node:url')
    const src = readFileSync(
      fileURLToPath(new URL('../components/domain/agent-desk/SessionComposer.tsx', import.meta.url)),
      'utf8'
    )
    // The single effect keyed on the session id, which is where this belongs.
    const start = src.indexOf('setStartFailure(null)')
    const end = src.indexOf('}, [sessionId])', start)
    expect(start, 'the session-change effect moved or was renamed').toBeGreaterThan(-1)
    expect(end, 'the session-change effect moved or was renamed').toBeGreaterThan(start)
    const body = src.slice(start, end)

    const missing = FLAGS.filter((f) => !body.includes(f))
    expect(missing, 'these would describe the previous chat after a swap').toEqual([])
  })
})

/**
 * Both surfaces that offer a mode must refuse the same ones.
 *
 * The new-chat cards took no `canWrite` at all, so on a Review or Explain
 * chat they offered Plan and Auto as freely selectable an inch above the
 * composer pills that correctly refused them. Clicking one lit it up while
 * the engine would refuse it -- a control that visibly accepts a choice it
 * cannot honour.
 */
describe('isModeBlocked', () => {
  it('refuses everything but Ask on a chat that only reads', () => {
    expect(isModeBlocked('Ask', false)).toBe(false)
    expect(isModeBlocked('Plan', false)).toBe(true)
    expect(isModeBlocked('Auto', false)).toBe(true)
  })

  it('refuses nothing on a chat that may change files', () => {
    for (const mode of ['Ask', 'Plan', 'Auto'] as const) {
      expect(isModeBlocked(mode, true)).toBe(false)
    }
  })

  /** Ask is the one mode a read-only chat can always be in. */
  it('always leaves Ask available', () => {
    expect(isModeBlocked('Ask', true)).toBe(false)
    expect(isModeBlocked('Ask', false)).toBe(false)
  })
})

/**
 * A team of helpers is only real in Plan and Auto.
 *
 * The backend adds the instruction that lets a lead hand work out only for
 * those two modes; in Ask it adds nothing, so the run is solo whatever was
 * chosen. Three separate controls offered a team without asking the mode --
 * the new-chat card, the composer's own line, and the team popover -- and the
 * team defaults to a team, so this was the state every read-only chat opened
 * in rather than one someone had to go looking for.
 */
describe('isTeamBlocked', () => {
  it('refuses a team in Ask, where helpers cannot exist', () => {
    expect(isTeamBlocked('Ask')).toBe(true)
  })

  it('allows a team in the modes that can actually hand work out', () => {
    expect(isTeamBlocked('Plan')).toBe(false)
    expect(isTeamBlocked('Auto')).toBe(false)
  })

  // The condition is the mode, not whether the chat may change files. A
  // read-only chat is covered because `isModeBlocked` already pins it to Ask,
  // but a chat that CAN write and is simply in Ask has the same problem --
  // gating on write permission would have left that one lying.
  it('covers a writable chat that is merely in Ask', () => {
    expect(isModeBlocked('Ask', true)).toBe(false)
    expect(isTeamBlocked('Ask')).toBe(true)
  })

  it('every read-only chat is covered, because it can only be in Ask', () => {
    for (const mode of ['Ask', 'Plan', 'Auto'] as const) {
      const reachable = !isModeBlocked(mode, false)
      if (reachable) expect(isTeamBlocked(mode)).toBe(true)
    }
  })

  it('says which modes to pick instead, in plain words', () => {
    expect(TEAM_NEEDS_MODE_REASON).toMatch(/Plan/)
    expect(TEAM_NEEDS_MODE_REASON).toMatch(/Auto/)
    expect(TEAM_NEEDS_MODE_REASON).not.toMatch(/mode-blocked|ExecutionTeam|graph instruction/)
  })
})
