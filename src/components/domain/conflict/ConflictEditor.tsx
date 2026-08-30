import { useEffect, useMemo, useRef } from 'react'
import { Compartment, EditorState, RangeSetBuilder, type Extension } from '@codemirror/state'
import {
  Decoration,
  EditorView,
  ViewPlugin,
  lineNumbers,
  type DecorationSet,
  type ViewUpdate,
} from '@codemirror/view'
import { HighlightStyle, syntaxHighlighting } from '@codemirror/language'
import { history, defaultKeymap, historyKeymap } from '@codemirror/commands'
import { keymap } from '@codemirror/view'
import { tags } from '@lezer/highlight'
import { loadLanguage } from '@/lib/codeLanguage'

/**
 * A CodeMirror pane for the conflict view: one of the two read-only sides, or
 * the editable result.
 *
 * Shares its colours and grammar loading with the Raw file viewer
 * (`domain/file/CodeViewer.tsx`) so a file looks the same wherever it is shown.
 * It is a separate component rather than a prop on that one because the needs
 * diverge past the point where one component could serve both honestly: this
 * one is editable, tints conflict regions, and reports its scroll position so
 * two panes can be kept in step.
 */

/**
 * Syntax colours, mapped onto the palette the rest of the app uses, so themes
 * added later are picked up without a per-theme mapping here.
 */
const highlight = HighlightStyle.define([
  { tag: [tags.keyword, tags.modifier, tags.controlKeyword], color: 'var(--gw-purple)' },
  {
    tag: [tags.comment, tags.lineComment, tags.blockComment],
    color: 'var(--gw-muted)',
    fontStyle: 'italic',
  },
  { tag: [tags.string, tags.special(tags.string), tags.regexp], color: 'var(--gw-green)' },
  { tag: [tags.number, tags.bool, tags.null, tags.atom], color: 'var(--gw-amber)' },
  {
    tag: [tags.function(tags.variableName), tags.function(tags.propertyName)],
    color: 'var(--gw-blue)',
  },
  { tag: [tags.typeName, tags.className, tags.namespace], color: 'var(--gw-accent-text)' },
  { tag: [tags.propertyName, tags.attributeName], color: 'var(--gw-blue)' },
  { tag: [tags.variableName, tags.definition(tags.variableName)], color: 'var(--gw-text)' },
  { tag: [tags.operator, tags.punctuation, tags.separator, tags.bracket], color: 'var(--gw-sub)' },
  { tag: [tags.tagName, tags.angleBracket], color: 'var(--gw-purple)' },
  { tag: tags.meta, color: 'var(--gw-sub)' },
  { tag: tags.link, color: 'var(--gw-blue)', textDecoration: 'underline' },
  { tag: tags.heading, color: 'var(--gw-accent-text)', fontWeight: '600' },
  { tag: tags.strong, fontWeight: '600' },
  { tag: tags.emphasis, fontStyle: 'italic' },
  { tag: tags.invalid, color: 'var(--gw-red)' },
])

const baseTheme = EditorView.theme({
  '&': {
    backgroundColor: 'var(--gw-bg)',
    color: 'var(--gw-text)',
    fontSize: '11px',
    height: '100%',
  },
  '&.cm-focused': { outline: 'none' },
  '.cm-scroller': {
    fontFamily: 'var(--font-mono)',
    lineHeight: '1.7',
    overflow: 'auto',
  },
  '.cm-content': { padding: '6px 0' },
  '.cm-gutters': {
    backgroundColor: 'var(--gw-bg)',
    color: 'var(--gw-muted)',
    border: 'none',
    borderRight: '1px solid var(--gw-border)',
    paddingRight: '5px',
  },
  '.cm-lineNumbers .cm-gutterElement': { paddingLeft: '8px' },
  '.cm-selectionBackground, &.cm-focused .cm-selectionBackground': {
    backgroundColor: 'var(--gw-accent)',
    opacity: '0.28',
  },

  // Conflict tinting. The same green/amber the diff viewer uses for added and
  // modified, at the same low alpha, so "ours" and "theirs" read as the same
  // pair of ideas everywhere in the app.
  '.cm-line.gw-ours': { backgroundColor: 'color-mix(in srgb, var(--gw-green) 9%, transparent)' },
  '.cm-line.gw-theirs': { backgroundColor: 'color-mix(in srgb, var(--gw-amber) 9%, transparent)' },
  '.cm-line.gw-base': { backgroundColor: 'color-mix(in srgb, var(--gw-muted) 10%, transparent)' },
  // Marker lines are structure, not content: dimmed and italic so the eye skips
  // them and lands on the code they delimit.
  '.cm-line.gw-marker': {
    backgroundColor: 'var(--gw-panel2)',
    color: 'var(--gw-muted)',
    fontStyle: 'italic',
  },
})

const readOnlyTheme = EditorView.theme({
  // Nothing here can be typed into, so a caret would only be a lie about what a
  // click does. Selection still works: reading a side usually ends in copying
  // part of it.
  '.cm-cursor, .cm-dropCursor': { display: 'none' },
})

const editableTheme = EditorView.theme({
  '.cm-activeLine': { backgroundColor: 'color-mix(in srgb, var(--gw-panel2) 60%, transparent)' },
  '.cm-activeLineGutter': {
    backgroundColor: 'var(--gw-panel2)',
    color: 'var(--gw-accent-text)',
  },
})

const OURS = Decoration.line({ class: 'gw-ours' })
const THEIRS = Decoration.line({ class: 'gw-theirs' })
const BASE = Decoration.line({ class: 'gw-base' })
const MARKER = Decoration.line({ class: 'gw-marker' })

/**
 * Tint the regions of a conflicted document.
 *
 * Recomputed from the document itself rather than from parsed sections, so it
 * stays correct while the user edits: delete a marker by hand and the tint
 * follows on the next keystroke. Lines are scanned rather than the text
 * re-parsed because CodeMirror hands us a line-oriented document already, and
 * this runs on every update.
 */
function conflictDecorations(view: EditorView): DecorationSet {
  const builder = new RangeSetBuilder<Decoration>()
  const doc = view.state.doc

  // 0 outside a conflict, 1 in ours, 2 in base, 3 in theirs.
  let region = 0
  for (let n = 1; n <= doc.lines; n++) {
    const line = doc.line(n)
    const text = line.text

    if (text.startsWith('<<<<<<<')) {
      region = 1
      builder.add(line.from, line.from, MARKER)
      continue
    }
    if (region !== 0 && text.startsWith('|||||||')) {
      region = 2
      builder.add(line.from, line.from, MARKER)
      continue
    }
    if (region !== 0 && text.startsWith('=======')) {
      region = 3
      builder.add(line.from, line.from, MARKER)
      continue
    }
    if (region !== 0 && text.startsWith('>>>>>>>')) {
      region = 0
      builder.add(line.from, line.from, MARKER)
      continue
    }

    if (region === 1) builder.add(line.from, line.from, OURS)
    else if (region === 2) builder.add(line.from, line.from, BASE)
    else if (region === 3) builder.add(line.from, line.from, THEIRS)
  }

  return builder.finish()
}

const conflictHighlighting = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet
    constructor(view: EditorView) {
      this.decorations = conflictDecorations(view)
    }
    update(update: ViewUpdate) {
      if (update.docChanged || update.viewportChanged) {
        this.decorations = conflictDecorations(update.view)
      }
    }
  },
  { decorations: (v) => v.decorations }
)

export interface ConflictEditorProps {
  text: string
  /** Picks the grammar. The file is never read from this path. */
  path: string
  /** Editable panes report every change; read-only panes omit this. */
  onChange?: (text: string) => void
  /** Tint `<<<<<<<` regions. Off for the single-side panes, which have none. */
  markers?: boolean
  /**
   * Which side this pane shows, for colouring its changed lines.
   *
   * Only the lines named by `changedLines` are tinted. Colouring the whole pane
   * would claim the entire file differs, when the point of showing both sides
   * is to find the few lines that actually do.
   */
  side?: 'ours' | 'theirs'
  /** 1-based line numbers that differ from the other side. */
  changedLines?: ReadonlySet<number>
  ariaLabel: string
  /** Reports scroll offset so two panes can be kept in step. */
  onScroll?: (scrollTop: number) => void
  /** Scroll offset to follow. Applied only when it differs, to avoid a loop. */
  scrollTop?: number
}

export function ConflictEditor({
  text,
  path,
  onChange,
  markers = false,
  side,
  changedLines,
  ariaLabel,
  onScroll,
  scrollTop,
}: ConflictEditorProps) {
  const host = useRef<HTMLDivElement>(null)
  const view = useRef<EditorView | null>(null)

  /**
   * The document to build with, kept current for the build effect.
   *
   * The build effect must not re-run when `text` changes -- a refetch that
   * returns the same file should not tear the editor down, losing the caret and
   * the undo history. StrictMode and Fast Refresh also mount effects twice, and
   * a captured first-render value would rebuild from a stale string.
   */
  const initialDoc = useRef(text)
  initialDoc.current = text

  /** Likewise for the change handler, so a new closure each render is free. */
  const changeHandler = useRef(onChange)
  changeHandler.current = onChange
  const scrollHandler = useRef(onScroll)
  scrollHandler.current = onScroll

  const editable = onChange != null

  /**
   * Tint only the lines that differ from the other side.
   *
   * Read through a ref so that changing which lines are marked does not rebuild
   * the editor; the plugin below re-reads it on each update.
   */
  const changed = useRef<ReadonlySet<number> | undefined>(changedLines)
  changed.current = changedLines

  const sideHighlighting = useMemo(() => {
    if (!side) return null
    const mark = Decoration.line({ class: side === 'ours' ? 'gw-ours' : 'gw-theirs' })
    const build = (view: EditorView) => {
      const builder = new RangeSetBuilder<Decoration>()
      const lines = changed.current
      if (lines && lines.size > 0) {
        const doc = view.state.doc
        for (let n = 1; n <= doc.lines; n++) {
          if (lines.has(n)) builder.add(doc.line(n).from, doc.line(n).from, mark)
        }
      }
      return builder.finish()
    }
    return ViewPlugin.fromClass(
      class {
        decorations: DecorationSet
        constructor(view: EditorView) {
          this.decorations = build(view)
        }
        update(update: ViewUpdate) {
          this.decorations = build(update.view)
        }
      },
      { decorations: (v) => v.decorations }
    )
  }, [side])

  useEffect(() => {
    if (!host.current) return

    // The grammar is swapped in after the editor exists, so it lives in a
    // compartment rather than the static extension list.
    const language = new Compartment()

    const extensions: Extension[] = [
      lineNumbers(),
      language.of([]),
      syntaxHighlighting(highlight),
      baseTheme,
      EditorView.contentAttributes.of({ 'aria-label': ariaLabel }),
    ]

    if (markers) extensions.push(conflictHighlighting)
    if (sideHighlighting) extensions.push(sideHighlighting)

    if (editable) {
      extensions.push(
        history(),
        // Undo/redo and the standard editing bindings. `defaultKeymap` last so
        // the history bindings win where they overlap.
        keymap.of([...historyKeymap, ...defaultKeymap]),
        editableTheme,
        EditorView.updateListener.of((update) => {
          if (update.docChanged) changeHandler.current?.(update.state.doc.toString())
        })
      )
    } else {
      extensions.push(
        EditorState.readOnly.of(true),
        EditorView.editable.of(false),
        readOnlyTheme
      )
    }

    if (scrollHandler.current) {
      extensions.push(
        EditorView.domEventHandlers({
          scroll: (_event, v) => {
            scrollHandler.current?.(v.scrollDOM.scrollTop)
            return false
          },
        })
      )
    }

    const editor = new EditorView({
      state: EditorState.create({ doc: initialDoc.current, extensions }),
      parent: host.current,
    })
    view.current = editor

    // The grammar arrives after the text. Showing the file immediately in plain
    // monospace and colouring it a moment later beats holding a blank pane
    // while a parser downloads.
    let cancelled = false
    void loadLanguage(path).then((lang) => {
      if (cancelled || !lang) return
      editor.dispatch({ effects: language.reconfigure(lang) })
    })

    return () => {
      cancelled = true
      editor.destroy()
      view.current = null
    }
    // `text` is deliberately absent: the effect below adopts changes to it
    // without rebuilding, so editing keeps its caret and undo history.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [path, editable, markers, sideHighlighting, ariaLabel])

  // The decoration plugin reads `changedLines` through a ref, so a new set does
  // not reach the view on its own. A no-op dispatch makes it recompute.
  useEffect(() => {
    const editor = view.current
    if (!editor || !side) return
    editor.dispatch({})
  }, [changedLines, side])

  // Adopt text that changed outside this editor -- a refetch, a mode switch, or
  // an AI draft being loaded in. Skipped when it already matches, so typing
  // does not dispatch its own change back into itself.
  useEffect(() => {
    const editor = view.current
    if (!editor) return
    const current = editor.state.doc.toString()
    if (current === text) return
    editor.dispatch({ changes: { from: 0, to: current.length, insert: text } })
  }, [text])

  // Follow a sibling pane's scroll. Guarded by a difference check so the two
  // panes cannot drive each other in a loop.
  useEffect(() => {
    const editor = view.current
    if (!editor || scrollTop == null) return
    if (Math.abs(editor.scrollDOM.scrollTop - scrollTop) < 1) return
    editor.scrollDOM.scrollTop = scrollTop
  }, [scrollTop])

  return <div ref={host} className="h-full min-h-0 overflow-hidden" />
}
