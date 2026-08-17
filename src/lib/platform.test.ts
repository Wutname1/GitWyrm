import { describe, expect, it, vi, afterEach } from 'vitest'
import { executableName, isLinux, isWindows } from '@/lib/platform'

function setPlatform(value: unknown) {
  vi.stubGlobal('navigator', { platform: value })
}

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('platform detection', () => {
  it('reports Windows for the 32-bit-looking value 64-bit Windows still returns', () => {
    setPlatform('Win32')
    expect(isWindows()).toBe(true)
    expect(isLinux()).toBe(false)
  })

  it('reports Linux for the WebKitGTK value', () => {
    setPlatform('Linux x86_64')
    expect(isLinux()).toBe(true)
    expect(isWindows()).toBe(false)
  })

  it('claims neither platform when navigator has no usable value', () => {
    // A missing platform must not read as Windows: that would show Windows-only
    // settings on Linux, which is the exact failure this module prevents.
    setPlatform(undefined)
    expect(isWindows()).toBe(false)
    expect(isLinux()).toBe(false)
  })

  it('appends .exe only on Windows', () => {
    setPlatform('Win32')
    expect(executableName('git')).toBe('git.exe')
    setPlatform('Linux x86_64')
    expect(executableName('git')).toBe('git')
  })
})
