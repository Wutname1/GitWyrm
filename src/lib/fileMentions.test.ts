import { describe, expect, it } from 'vitest'
import { applyFileMention, fileLabel, matchFileMention } from './fileMentions'

const FILES = ['src/App.tsx', 'src/lib/appState.ts', 'README.md', 'docs/app/overview.md']

describe('file mentions', () => {
  it('opens after a space or at the start, not inside a word', () => {
    expect(matchFileMention(FILES, '@')?.matches).toHaveLength(FILES.length)
    expect(matchFileMention(FILES, 'look at @rea')?.matches).toEqual(['README.md'])
    expect(matchFileMention(FILES, 'mail me@app')).toBeNull()
  })

  it('closes once a space follows the mention', () => {
    expect(matchFileMention(FILES, '@README.md and')).toBeNull()
  })

  it('ranks file names that start with the text above paths that contain it', () => {
    const state = matchFileMention(FILES, 'see @app')!
    expect(state.matches.slice(0, 2)).toEqual(['src/App.tsx', 'src/lib/appState.ts'])
    expect(state.matches).toContain('docs/app/overview.md')
  })

  it('writes the chosen path and a space', () => {
    const draft = 'see @ap please'
    const state = matchFileMention(FILES, draft, 7)!
    expect(applyFileMention(draft, state, 'src/App.tsx', 7)).toBe('see @src/App.tsx please')
  })

  it('labels a chip with the last part of a path', () => {
    expect(fileLabel('C:\\notes\\plan.md')).toBe('plan.md')
    expect(fileLabel('src/lib/')).toBe('lib')
  })
})
