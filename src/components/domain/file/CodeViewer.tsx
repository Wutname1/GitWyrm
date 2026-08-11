import { useEffect, useRef } from 'react'
import { Compartment, EditorState, type Extension } from '@codemirror/state'
import { EditorView, lineNumbers, highlightActiveLine } from '@codemirror/view'
import { HighlightStyle, syntaxHighlighting } from '@codemirror/language'
import { tags } from '@lezer/highlight'
import { loadLanguage } from '@/lib/codeLanguage'

/**
 * A read-only CodeMirror view of one file, used by the Raw file view.
 *
 * Read-only in the strong sense: the document cannot be edited, and the caret
 * is hidden. Selection and copy still work, because looking at a file at a
 * point in time usually ends with taking a piece of it somewhere else.
 *
 * Colours come from the app's `--gw-*` custom properties rather than a packaged
 * CodeMirror theme, so the viewer follows every theme -- including ones added
 * later -- without a per-theme mapping to maintain.
 */

/**
 * Syntax colours, mapped onto the palette the rest of the app already uses.
 *
 * Kept to the distinctions that carry meaning while reading -- keywords,
 * strings, comments, names, numbers -- rather than colouring every token type
 * the grammar can name.
 */
const highlight = HighlightStyle.define([
  { tag: [tags.keyword, tags.modifier, tags.controlKeyword], color: 'var(--gw-purple)' },
  { tag: [tags.comment, tags.lineComment, tags.blockComment], color: 'var(--gw-muted)', fontStyle: 'italic' },
  { tag: [tags.string, tags.special(tags.string), tags.regexp], color: 'var(--gw-green)' },
  { tag: [tags.number, tags.bool, tags.null, tags.atom], color: 'var(--gw-amber)' },
  { tag: [tags.function(tags.variableName), tags.function(tags.propertyName)], color: 'var(--gw-blue)' },
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

const theme = EditorView.theme({
  '&': {
    backgroundColor: 'var(--gw-bg)',
    color: 'var(--gw-text)',
    fontSize: '12px',
    height: '100%',
  },
  '&.cm-focused': { outline: 'none' },
  '.cm-scroller': {
    fontFamily: 'var(--font-mono)',
    lineHeight: '1.55',
    overflow: 'auto',
  },
  '.cm-content': { padding: '8px 0' },
  '.cm-gutters': {
    backgroundColor: 'var(--gw-bg)',
    color: 'var(--gw-muted)',
    border: 'none',
    borderRight: '1px solid var(--gw-border)',
    paddingRight: '6px',
  },
  '.cm-lineNumbers .cm-gutterElement': { paddingLeft: '10px' },
  '.cm-activeLine': { backgroundColor: 'var(--gw-panel2)' },
  '.cm-activeLineGutter': {
    backgroundColor: 'var(--gw-panel2)',
    color: 'var(--gw-accent-text)',
  },
  // Nothing here can be typed into, so a caret would only ever be a lie about
  // what a click does.
  '.cm-cursor, .cm-dropCursor': { display: 'none' },
  '.cm-selectionBackground, &.cm-focused .cm-selectionBackground': {
    backgroundColor: 'var(--gw-accent)',
    opacity: '0.28',
  },
  '.cm-content ::selection': { backgroundColor: 'transparent' },
})

export function CodeViewer({
  text,
  /** Used to pick the grammar; the file is never read from this path. */
  path,
  /** Off by default: code lines are long and horizontal scrolling preserves them. */
  wrap = false,
  ariaLabel,
}: {
  text: string
  path: string
  wrap?: boolean
  ariaLabel: string
}) {
  const host = useRef<HTMLDivElement>(null)
  const view = useRef<EditorView | null>(null)
  /**
   * The document to build with, kept current for the build effect below.
   *
   * The build effect must not re-run when `text` changes -- a refetch that
   * returns the same file should not tear the editor down -- so it reads
   * through a ref. StrictMode and Fast Refresh also mount effects twice, and a
   * captured first-render value would rebuild the viewer from an empty string.
   */
  const initialDoc = useRef(text)
  initialDoc.current = text

  useEffect(() => {
    if (!host.current) return

    // The grammar is swapped in after the editor exists, so it lives in a
    // compartment rather than the static extension list.
    const language = new Compartment()

    const extensions: Extension[] = [
      lineNumbers(),
      highlightActiveLine(),
      language.of([]),
      syntaxHighlighting(highlight),
      EditorState.readOnly.of(true),
      EditorView.editable.of(false),
      theme,
      EditorView.contentAttributes.of({ 'aria-label': ariaLabel }),
    ]
    if (wrap) extensions.push(EditorView.lineWrapping)

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
    // Rebuilt when the file identity or wrapping changes; `text` is handled by
    // the effect below so that a refetch keeps the scroll position.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [path, wrap])

  // Adopt text that changed outside the viewer -- a refetch after the file was
  // edited on disk, or the same view being pointed at a different revision.
  useEffect(() => {
    const editor = view.current
    if (!editor) return
    const current = editor.state.doc.toString()
    if (current === text) return
    editor.dispatch({ changes: { from: 0, to: current.length, insert: text } })
  }, [text])

  return <div ref={host} className="h-full min-h-0 overflow-hidden" />
}
