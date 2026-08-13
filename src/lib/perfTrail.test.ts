import { beforeEach, describe, expect, it } from 'vitest'
import { perfMarksText, readPerfMarks, recordPerfMark, resetPerfMarks, timed } from './perfTrail'

beforeEach(() => resetPerfMarks())

describe('perf trail', () => {
  it('records what was measured and how long it took', () => {
    recordPerfMark('openRepo', 1234.6)
    const [mark] = readPerfMarks()
    expect(mark.label).toBe('openRepo')
    // Rounded: sub-millisecond precision says nothing about felt latency.
    expect(mark.ms).toBe(1235)
  })

  it('keeps details as scalars for the report', () => {
    recordPerfMark('openRepo', 10, { cold: true })
    expect(readPerfMarks()[0].detail).toEqual({ cold: true })
  })

  it('drops the oldest marks rather than growing without bound', () => {
    for (let i = 0; i < 60; i++) recordPerfMark(`mark-${i}`, i)
    const all = readPerfMarks()
    expect(all.length).toBe(40)
    // The newest survive; the oldest fall off the front.
    expect(all[all.length - 1].label).toBe('mark-59')
    expect(all.some((m) => m.label === 'mark-0')).toBe(false)
  })

  it('times a call and still returns its value', async () => {
    const value = await timed('work', async () => 'done')
    expect(value).toBe('done')
    expect(readPerfMarks()[0].label).toBe('work')
  })

  it('records a failed call too, then rethrows', async () => {
    // A report about something that went wrong is exactly when the timing
    // matters, so a throw must not lose the mark.
    await expect(timed('work', async () => Promise.reject(new Error('boom')))).rejects.toThrow(
      'boom'
    )
    expect(readPerfMarks()[0].label).toBe('work')
  })

  it('renders nothing when there is nothing to report', () => {
    // Lets the caller omit the section entirely instead of attaching an empty
    // heading to the bug report.
    expect(perfMarksText()).toBe('')
  })

  it('renders a readable line per mark, with details', () => {
    recordPerfMark('openRepo', 8000, { cold: true })
    const text = perfMarksText()
    expect(text).toContain('openRepo')
    expect(text).toContain('8000ms')
    expect(text).toContain('cold=true')
  })
})
