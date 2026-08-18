import { describe, expect, it, vi, afterEach } from 'vitest'
import { readSystemScheme, resolveMode } from '@/lib/themes'

/** Stub matchMedia so a given query set matches. */
function stubMedia(matching: string[]) {
  vi.stubGlobal('window', {
    matchMedia: (q: string) => ({ matches: matching.some((m) => q.includes(m)) }),
  })
}

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('readSystemScheme', () => {
  it('reads a real dark preference', () => {
    stubMedia(['dark'])
    expect(readSystemScheme()).toBe('dark')
  })

  it('reads a real light preference', () => {
    stubMedia(['light'])
    expect(readSystemScheme()).toBe('light')
  })

  it('reports unknown when the OS matches neither', () => {
    // GNOME sits here until someone changes color-scheme from 'default', and a
    // desktop with no XDG appearance portal never answers at all.
    stubMedia([])
    expect(readSystemScheme()).toBe('unknown')
  })

  it('reports unknown when matchMedia is missing entirely', () => {
    vi.stubGlobal('window', {})
    expect(readSystemScheme()).toBe('unknown')
  })
})

describe('resolveMode', () => {
  it('honours an explicit choice regardless of the OS', () => {
    expect(resolveMode('light', 'dark')).toBe('light')
    expect(resolveMode('dark', 'light')).toBe('dark')
  })

  it('follows the OS when it has a preference', () => {
    expect(resolveMode('system', 'dark')).toBe('dark')
    expect(resolveMode('system', 'light')).toBe('light')
  })

  it('falls to dark when the OS expressed no preference', () => {
    // The window background is painted #121212 before any JavaScript runs, so
    // resolving a silent OS to light would contradict the frame already on
    // screen - and it would do it on the desktops least likely to answer.
    expect(resolveMode('system', 'unknown')).toBe('dark')
  })
})
