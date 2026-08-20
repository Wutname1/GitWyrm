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
