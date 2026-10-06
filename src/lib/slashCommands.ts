/**
 * Slash commands in the Agent Desk message box.
 *
 * The commands themselves come from the AI tool a chat runs (its skills,
 * custom commands and built-ins), never from a list written here: a menu that
 * offers what the tool cannot do is worse than no menu. This file only decides
 * which of them match what has been typed, and what accepting one does to the
 * draft.
 */

/** One command a tool offers. */
export interface SlashCommand {
  /** Without the leading slash. May contain a space for a sub-command ("impeccable audit"). */
  name: string
  description: string
  /** What sort of thing it is, shown on the right of its row. */
  kind: 'skill' | 'command' | 'builtin' | 'prompt'
  /** What to type after it, when the tool says ("<issue number>"). */
  argumentHint?: string | null
}

export interface SlashMatch {
  command: SlashCommand
  /** Where the typed text matched inside `command.name`, for highlighting. */
  matchStart: number
  matchLength: number
}

export interface SlashQuery {
  /** What follows the slash, up to the caret. */
  query: string
  matches: SlashMatch[]
}

/** How many rows the menu shows before it scrolls. */
export const SLASH_MENU_LIMIT = 50

/**
 * The menu's state for a draft, or `null` when no menu should show.
 *
 * Only a slash at the very start of the message opens it, the way every
 * agent tool reads one: a slash later in a sentence ("and/or", a path) is just
 * text. The menu stays open through spaces so sub-commands like
 * "impeccable audit" can be reached, and closes once nothing matches.
 */
export function matchSlashCommands(
  commands: readonly SlashCommand[],
  draft: string,
  caret: number = draft.length
): SlashQuery | null {
  if (!draft.startsWith('/')) return null
  const typed = draft.slice(1, Math.max(1, caret))
  // A newline means the person has moved on to writing the message itself.
  if (typed.includes('\n')) return null
  const query = typed.toLowerCase()

  const prefix: SlashMatch[] = []
  const inside: SlashMatch[] = []
  for (const command of commands) {
    const name = command.name.toLowerCase()
    if (name.startsWith(query)) {
      prefix.push({ command, matchStart: 0, matchLength: query.length })
    } else if (query.length > 0 && !query.includes(' ')) {
      const at = name.indexOf(query)
      if (at > 0) inside.push({ command, matchStart: at, matchLength: query.length })
    }
  }
  const byName = (a: SlashMatch, b: SlashMatch) =>
    a.command.name.length - b.command.name.length || a.command.name.localeCompare(b.command.name)
  prefix.sort(byName)
  inside.sort(byName)
  const matches = [...prefix, ...inside].slice(0, SLASH_MENU_LIMIT)

  // Typed a whole command and then a space: the person is writing its
  // arguments now, and a menu still hovering over the box is in the way --
  // unless longer sub-commands still match what they typed.
  if (matches.length === 0) return null
  if (typed.endsWith(' ') && matches.every((m) => m.command.name.toLowerCase() === query.trimEnd())) {
    return null
  }
  return { query: typed, matches }
}

/**
 * The draft after accepting a command: the slash, its name and one space,
 * keeping anything already written after the part being completed.
 */
export function applySlashCommand(draft: string, command: SlashCommand, caret: number = draft.length): string {
  const rest = draft.slice(caret).replace(/^\S*/, '').replace(/^ /, '')
  return `/${command.name} ${rest}`
}

/** The word shown on the right of a row. */
export function slashKindLabel(kind: SlashCommand['kind']): string {
  switch (kind) {
    case 'skill':
      return 'Skill'
    case 'command':
      return 'Command'
    case 'prompt':
      return 'Prompt'
    case 'builtin':
      return 'Built in'
  }
}
