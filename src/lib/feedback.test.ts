import { describe, expect, it } from 'vitest'
import { attachmentsFor, bugReportMarkdown, type Diagnostics } from './feedback'

const diagnostics = (over: Partial<Diagnostics> = {}): Diagnostics => ({
  version: '1.2.3',
  buildDate: '2026-08-17',
  gitHash: 'abc1234',
  debug: false,
  platform: 'Win32',
  userAgent: 'test',
  logTail: 'line one\nline two',
  perfTrail: '1.0s ago  openRepo  20ms',
  ...over,
})

/** A 1x1 PNG, small enough to inline and real enough to decode. */
const PNG_DATA_URL =
  'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg=='

describe('report attachments', () => {
  it('carries the log and timings when the user leaves them on', () => {
    const files = attachmentsFor(diagnostics(), { includeLog: true })
    expect(files?.map((f) => f.filename)).toEqual(['gitwyrm.log', 'timings.txt'])
  })

  it('sends nothing about the machine when the user opts out', () => {
    // Both drop together: the opt-out would be a half-truth if the timings,
    // which also describe the machine, kept riding along.
    expect(attachmentsFor(diagnostics(), { includeLog: false })).toBeUndefined()
  })

  it('attaches an opted-out report screenshot on its own', () => {
    const files = attachmentsFor(diagnostics(), {
      includeLog: false,
      screenshot: { dataUrl: PNG_DATA_URL, name: 'shot.png' },
    })
    expect(files?.map((f) => f.filename)).toEqual(['screenshot.png'])
  })

  it('sends the screenshot as bytes so it downloads as an image', () => {
    const [file] = attachmentsFor(diagnostics({ logTail: '', perfTrail: '' }), {
      includeLog: true,
      screenshot: { dataUrl: PNG_DATA_URL, name: 'shot.png' },
    })!
    expect(file.data).toBeInstanceOf(Uint8Array)
    expect(file.contentType).toBe('image/png')
    // PNG magic number: proof the base64 was really decoded, not passed through.
    expect(Array.from((file.data as Uint8Array).slice(0, 4))).toEqual([137, 80, 78, 71])
  })

  it('discards the user filename, which can name a client or a person', () => {
    const [file] = attachmentsFor(diagnostics({ logTail: '', perfTrail: '' }), {
      includeLog: true,
      screenshot: { dataUrl: PNG_DATA_URL, name: 'AcmeCorp-merger-plan.png' },
    })!
    expect(file.filename).toBe('screenshot.png')
  })

  it('skips a screenshot it cannot decode rather than sending garbage', () => {
    const files = attachmentsFor(diagnostics({ logTail: '', perfTrail: '' }), {
      includeLog: true,
      screenshot: { dataUrl: 'not-a-data-url', name: 'shot.png' },
    })
    expect(files).toBeUndefined()
  })
})

describe('report markdown', () => {
  it('leaves the log out of a feedback note, which has no use for it', () => {
    const body = bugReportMarkdown('an idea', diagnostics(), true, 'feedback')
    expect(body).toContain('## Feedback')
    expect(body).not.toContain('## Log')
    expect(body).not.toContain('## Recent timings')
  })

  it('keeps the log in a bug report', () => {
    const body = bugReportMarkdown('it broke', diagnostics(), true, 'bug')
    expect(body).toContain('## What happened')
    expect(body).toContain('line one')
  })
})
