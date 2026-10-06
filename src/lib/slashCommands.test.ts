import { describe, expect, it } from 'vitest'
import { applySlashCommand, matchSlashCommands, type SlashCommand } from './slashCommands'

const cmd = (name: string, kind: SlashCommand['kind'] = 'skill'): SlashCommand => ({ name, description: '', kind })
const COMMANDS = [cmd('impeccable'), cmd('impeccable audit'), cmd('impeccable live'), cmd('goal', 'command'), cmd('review', 'builtin')]

describe('slash command matching', () => {
  it('only opens for a slash at the very start', () => {
    expect(matchSlashCommands(COMMANDS, 'fix and/or test')).toBeNull()
    expect(matchSlashCommands(COMMANDS, ' /goal')).toBeNull()
    expect(matchSlashCommands(COMMANDS, '/')?.matches).toHaveLength(COMMANDS.length)
  })

  it('puts names that start with the text first, shortest first', () => {
    const names = matchSlashCommands(COMMANDS, '/impec')!.matches.map((m) => m.command.name)
    expect(names).toEqual(['impeccable', 'impeccable live', 'impeccable audit'])
  })

  it('also finds a word inside a name, after the prefix matches', () => {
    const state = matchSlashCommands(COMMANDS, '/audit')!
    expect(state.matches[0].command.name).toBe('impeccable audit')
    expect(state.matches[0].matchStart).toBe('impeccable '.length)
  })

  it('reaches sub-commands through a space', () => {
    const names = matchSlashCommands(COMMANDS, '/impeccable a')!.matches.map((m) => m.command.name)
    expect(names).toEqual(['impeccable audit'])
  })

  it('gets out of the way once a whole command and a space are typed', () => {
    expect(matchSlashCommands(COMMANDS, '/goal ')).toBeNull()
    expect(matchSlashCommands(COMMANDS, '/goal finish the merge')).toBeNull()
  })

  it('stays open after a command that has sub-commands', () => {
    expect(matchSlashCommands(COMMANDS, '/impeccable ')?.matches.map((m) => m.command.name)).toEqual([
      'impeccable live',
      'impeccable audit',
    ])
  })

  it('closes when nothing matches, and on a new line', () => {
    expect(matchSlashCommands(COMMANDS, '/zzz')).toBeNull()
    expect(matchSlashCommands(COMMANDS, '/go\nmore')).toBeNull()
  })

  it('is case-insensitive', () => {
    expect(matchSlashCommands(COMMANDS, '/GO')?.matches[0].command.name).toBe('goal')
  })

  it('accepting a command writes it with one trailing space', () => {
    expect(applySlashCommand('/imp', cmd('impeccable'))).toBe('/impeccable ')
    expect(applySlashCommand('/go the rest', cmd('goal'), 3)).toBe('/goal the rest')
  })
})
