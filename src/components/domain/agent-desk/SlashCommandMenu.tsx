import { useEffect, useRef } from 'react'
import { cn } from '@/lib/utils'
import { slashKindLabel, type SlashMatch, type SlashQuery } from '@/lib/slashCommands'
import type { FileMentionQuery } from '@/lib/fileMentions'

/**
 * The list that opens above the message box when a message starts with "/".
 *
 * Keyboard handling lives in the composer, which owns the textarea: Up and
 * Down move `activeIndex`, Tab or Enter accept, Escape closes. This only
 * draws the rows and answers clicks, so the caret never leaves the box.
 */
export function SlashCommandMenu({
  state,
  activeIndex,
  onPick,
  onHover,
  listId,
}: {
  state: SlashQuery
  activeIndex: number
  onPick: (match: SlashMatch) => void
  onHover: (index: number) => void
  /** Ties the list to the textarea for screen readers (`aria-controls`). */
  listId: string
}) {
  const listRef = useRef<HTMLDivElement>(null)

  // Keep the highlighted row in view while arrowing through a long list.
  useEffect(() => {
    const row = listRef.current?.querySelector<HTMLElement>(`[data-index="${activeIndex}"]`)
    row?.scrollIntoView({ block: 'nearest' })
  }, [activeIndex])

  return (
    <div
      ref={listRef}
      id={listId}
      role="listbox"
      aria-label="Slash commands"
      className="absolute bottom-full left-0 right-0 z-20 mb-1.5 max-h-[16rem] overflow-y-auto rounded-lg border border-border bg-panel p-1 shadow-lg"
    >
      {state.matches.map((match, index) => {
        const { command, matchStart, matchLength } = match
        const active = index === activeIndex
        return (
          <div
            key={command.name}
            id={`${listId}-${index}`}
            data-index={index}
            role="option"
            aria-selected={active}
            // mousedown, not click: a click would blur the textarea first.
            onMouseDown={(e) => {
              e.preventDefault()
              onPick(match)
            }}
            onMouseEnter={() => onHover(index)}
            className={cn(
              'flex cursor-pointer items-baseline gap-2.5 rounded-md border-l-2 px-2 py-1 text-xs',
              active ? 'border-primary bg-panel3' : 'border-transparent hover:bg-panel2'
            )}
          >
            <span className="flex-none font-semibold text-foreground">
              /{command.name.slice(0, matchStart)}
              <span className="text-accent-text">{command.name.slice(matchStart, matchStart + matchLength)}</span>
              {command.name.slice(matchStart + matchLength)}
              {command.argumentHint && (
                <span className="ml-1 font-normal text-muted-foreground">{command.argumentHint}</span>
              )}
            </span>
            <span className="min-w-0 flex-1 truncate text-2xs text-muted-foreground">{command.description}</span>
            <span className="flex-none text-2xs text-muted-foreground">{slashKindLabel(command.kind)}</span>
          </div>
        )
      })}
    </div>
  )
}

/** The project-file list for an "@" mention, drawn like the slash menu. */
export function FileMentionMenu({
  state,
  activeIndex,
  onPick,
  onHover,
  listId,
}: {
  state: FileMentionQuery
  activeIndex: number
  onPick: (path: string) => void
  onHover: (index: number) => void
  listId: string
}) {
  const listRef = useRef<HTMLDivElement>(null)
  useEffect(() => {
    const row = listRef.current?.querySelector<HTMLElement>(`[data-index="${activeIndex}"]`)
    row?.scrollIntoView({ block: 'nearest' })
  }, [activeIndex])

  return (
    <div
      ref={listRef}
      id={listId}
      role="listbox"
      aria-label="Project files"
      className="absolute bottom-full left-0 right-0 z-20 mb-1.5 max-h-[16rem] overflow-y-auto rounded-lg border border-border bg-panel p-1 shadow-lg"
    >
      {state.matches.map((path, index) => {
        const slash = path.lastIndexOf('/')
        const name = path.slice(slash + 1)
        const folder = slash > 0 ? path.slice(0, slash) : ''
        return (
          <div
            key={path}
            id={`${listId}-${index}`}
            data-index={index}
            role="option"
            aria-selected={index === activeIndex}
            onMouseDown={(e) => {
              e.preventDefault()
              onPick(path)
            }}
            onMouseEnter={() => onHover(index)}
            className={cn(
              'flex cursor-pointer items-baseline gap-2.5 rounded-md border-l-2 px-2 py-1 text-xs',
              index === activeIndex ? 'border-primary bg-panel3' : 'border-transparent hover:bg-panel2'
            )}
          >
            <span className="flex-none font-semibold text-foreground">{name}</span>
            <span className="min-w-0 flex-1 truncate text-2xs text-muted-foreground">{folder}</span>
          </div>
        )
      })}
    </div>
  )
}
