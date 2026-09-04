import { describe, expect, it } from 'vitest'
import { parsePlanChecklist } from './agentDeskPlan'

describe('parsePlanChecklist', () => {
  it('parses a mix of done, working, and pending rows with owner labels', () => {
    const text = [
      '- [x] Trace the failed settings read (Luna - done)',
      '- [~] Add a safe recovery state (Fable - working)',
      '- [ ] Run settings and branch checks (Luna - waiting)',
      '- [ ] Review the combined change (Sol - queued)',
    ].join('\n')

    const rows = parsePlanChecklist(text)
    expect(rows).toHaveLength(4)
    expect(rows[0]).toEqual({ text: 'Trace the failed settings read', state: 'done', owner: 'Luna - done' })
    expect(rows[1]).toEqual({ text: 'Add a safe recovery state', state: 'working', owner: 'Fable - working' })
    expect(rows[2]).toEqual({ text: 'Run settings and branch checks', state: 'pending', owner: 'Luna - waiting' })
    expect(rows[3]).toEqual({ text: 'Review the combined change', state: 'pending', owner: 'Sol - queued' })
  })

  it('supports the review variant with confidence/severity labels instead of an owner', () => {
    const text = [
      '- [~] Repository switch race (high confidence)',
      '- [x] Recovery copy and focus behavior (looks good)',
    ].join('\n')

    const rows = parsePlanChecklist(text)
    expect(rows[0]).toEqual({ text: 'Repository switch race', state: 'working', owner: 'high confidence' })
    expect(rows[1]).toEqual({ text: 'Recovery copy and focus behavior', state: 'done', owner: 'looks good' })
  })

  it('parses a checklist line with no trailing owner', () => {
    const rows = parsePlanChecklist('- [ ] Ship the release')
    expect(rows).toEqual([{ text: 'Ship the release', state: 'pending', owner: null }])
  })

  it('is case-insensitive for the done mark and accepts asterisk bullets', () => {
    const rows = parsePlanChecklist('* [X] Done thing')
    expect(rows).toEqual([{ text: 'Done thing', state: 'done', owner: null }])
  })

  it('ignores non-checklist lines interspersed with checklist lines', () => {
    const text = ['I found the crash path and made a plan.', '- [ ] Step one', '', 'Some trailing note.'].join('\n')
    const rows = parsePlanChecklist(text)
    expect(rows).toEqual([{ text: 'Step one', state: 'pending', owner: null }])
  })

  it('returns an empty list for plain prose with no checklist lines', () => {
    expect(parsePlanChecklist('Just a normal reply, nothing structured here.')).toEqual([])
  })

  it('drops a checklist line whose step text is empty after removing the owner', () => {
    expect(parsePlanChecklist('- [ ] (Luna - done)')).toEqual([])
  })
})

describe('a checklist inside a code fence is an example, not a plan', () => {
  // The agent quoting the convention it is meant to follow, or quoting a
  // tasks file, produced a real plan card with working status icons -- green
  // ticks and a spinning "in progress" -- attached to a message that never
  // claimed to be reporting a plan.
  //
  // The OpenSpec parser that reads the same syntax tracks fences
  // (`openspec/parse.rs`); this one did not. Same convention, two parsers,
  // one of them thought through.
  it('ignores checklist lines between fences', () => {
    const text = [
      'Here is the format I will use:',
      '```',
      '- [x] A finished step',
      '- [ ] A pending step',
      '```',
      'I have not started yet.',
    ].join('\n')
    expect(parsePlanChecklist(text)).toEqual([])
  })

  it('still reads a real checklist after a fenced block', () => {
    const text = [
      'For example:',
      '```',
      '- [ ] not mine',
      '```',
      '- [x] Actually done',
    ].join('\n')
    const rows = parsePlanChecklist(text)
    expect(rows).toHaveLength(1)
    expect(rows[0].text).toBe('Actually done')
  })

  it('does not swallow the rest of a message when a fence never closes', () => {
    // An unterminated fence must not hide every later line: the safer
    // reading is that the agent forgot to close it, not that everything
    // after it is an example.
    const text = ['```', '- [x] Inside an unclosed fence'].join('\n')
    expect(parsePlanChecklist(text)).toEqual([])
  })
})
