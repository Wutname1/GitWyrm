import { describe, expect, it } from 'vitest'
import { joinPath, normalizePath, pathKey, pathName, samePath } from './paths'

describe('native path handling', () => {
  it('keeps POSIX paths and their case', () => {
    expect(normalizePath('/home/vboxuser/Documents/gitwyrm/')).toBe(
      '/home/vboxuser/Documents/gitwyrm',
    )
    expect(pathKey('/home/Me/Repo')).not.toBe(pathKey('/home/me/repo'))
  })

  it('repairs Linux paths corrupted by the old Windows-only normalizer', () => {
    expect(normalizePath('\\home\\vboxuser\\Documents\\gitwyrm')).toBe(
      '/home/vboxuser/Documents/gitwyrm',
    )
  })

  it('keeps Windows paths case-insensitive and preserves UNC roots', () => {
    expect(normalizePath('C:/Code/GitWyrm/')).toBe('C:\\Code\\GitWyrm')
    expect(normalizePath('\\\\server\\share\\repo\\')).toBe('\\\\server\\share\\repo')
    expect(samePath('C:/Code/GitWyrm', 'c:\\code\\gitwyrm\\')).toBe(true)
  })

  it('gets a short name from either path style', () => {
    expect(pathName('/home/vboxuser/Documents/gitwyrm')).toBe('gitwyrm')
    expect(pathName('C:\\code\\GitWyrm')).toBe('GitWyrm')
  })

  it('joins with the base path separator', () => {
    expect(joinPath('/home/vboxuser/Documents', 'gitwyrm')).toBe(
      '/home/vboxuser/Documents/gitwyrm',
    )
    expect(joinPath('C:\\code', 'GitWyrm')).toBe('C:\\code\\GitWyrm')
  })
})
