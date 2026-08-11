import { describe, expect, it } from 'vitest'
import { languageLabel } from './codeLanguage'

describe('languageLabel', () => {
  it('reads the extension, not the rest of the path', () => {
    expect(languageLabel('src/components/App.tsx')).toBe('TypeScript')
    expect(languageLabel('src-tauri/src/commands/file.rs')).toBe('Rust')
    expect(languageLabel('deep/nested/dir.rs/thing.py')).toBe('Python')
  })

  it('takes the last extension on a multi-part name', () => {
    expect(languageLabel('types/bindings.d.ts')).toBe('TypeScript')
    expect(languageLabel('lib/codeLanguage.test.ts')).toBe('TypeScript')
  })

  it('handles files whose name is the language', () => {
    expect(languageLabel('Dockerfile')).toBe('Dockerfile')
    expect(languageLabel('docker/Dockerfile.prod')).toBe('Dockerfile')
    expect(languageLabel('Makefile')).toBe('Shell')
    expect(languageLabel('.gitignore')).toBe('INI')
  })

  it('handles Windows separators', () => {
    expect(languageLabel('src\\lib\\utils.ts')).toBe('TypeScript')
  })

  it('returns null for anything with no grammar', () => {
    expect(languageLabel('LICENSE')).toBeNull()
    expect(languageLabel('assets/logo.png')).toBeNull()
    expect(languageLabel('')).toBeNull()
  })
})
