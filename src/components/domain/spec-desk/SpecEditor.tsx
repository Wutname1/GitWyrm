import { useEffect, useRef } from 'react'
import { EditorState } from '@codemirror/state'
import { EditorView, keymap, highlightActiveLine, lineNumbers } from '@codemirror/view'
import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands'
import { markdown } from '@codemirror/lang-markdown'
import { HighlightStyle, syntaxHighlighting } from '@codemirror/language'
import { tags } from '@lezer/highlight'

/**
 * The markdown editor for spec files.
 *
 * CodeMirror rather than Monaco: these files are prose, and Monaco brings an
 * IDE's weight and chrome (workers, minimap, IntelliSense) to editing a
 * paragraph. This is a text box that knows markdown, which is all the job needs.
 *
 * Colours come from the app's `--gw-*` custom properties rather than a packaged
 * CodeMirror theme, so the editor follows every theme -- including ones added
 * later -- without a per-theme mapping to maintain.
 */

/**
 * Markdown highlighting, in the app's own palette.
 *
 * Deliberately restrained: headings, emphasis, links, code and quotes. Spec
 * files are read as prose, and colouring every token turns a proposal into a
 * christmas tree that is harder to read than plain text.
 */
const highlight = HighlightStyle.define([
  { tag: tags.heading, color: 'var(--gw-accent-text)', fontWeight: '600' },
  { tag: tags.strong, color: 'var(--gw-text)', fontWeight: '600' },
  { tag: tags.emphasis, color: 'var(--gw-text)', fontStyle: 'italic' },
  { tag: tags.link, color: 'var(--gw-blue)', textDecoration: 'underline' },
  { tag: tags.url, color: 'var(--gw-blue)' },
  { tag: tags.monospace, color: 'var(--gw-amber)' },
  { tag: tags.quote, color: 'var(--gw-sub)', fontStyle: 'italic' },
  { tag: tags.list, color: 'var(--gw-accent-text)' },
  { tag: tags.contentSeparator, color: 'var(--gw-border)' },
  { tag: tags.processingInstruction, color: 'var(--gw-muted)' },
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
    lineHeight: '1.6',
    overflow: 'auto',
  },
  '.cm-content': { padding: '10px 0' },
  '.cm-gutters': {
    backgroundColor: 'var(--gw-bg)',
    color: 'var(--gw-muted)',
    border: 'none',
    paddingRight: '4px',
  },
  // The active line is a neutral lift, not an accent tint: the accent is what
  // marks a selection, and using it for both made the two indistinguishable.
  '.cm-activeLine': { backgroundColor: 'var(--gw-panel2)' },
  '.cm-activeLineGutter': {
    backgroundColor: 'var(--gw-panel2)',
    color: 'var(--gw-accent-text)',
    fontWeight: '600',
  },
  // A 1px caret in the accent colour disappears against syntax-highlighted
  // text. Wider, brighter, and always drawn even when the view is unfocused.
  '.cm-cursor, .cm-dropCursor': {
    borderLeft: '2px solid var(--gw-text)',
    marginLeft: '-1px',
  },
  '&.cm-focused .cm-cursor': { borderLeftColor: 'var(--gw-accent-text)' },
  // Selection needs to read as a block behind the text. CodeMirror draws its
  // own layer, so the native ::selection has to be transparent or the two
  // stack into a muddy double-tint.
  '.cm-selectionBackground, &.cm-focused .cm-selectionBackground': {
    backgroundColor: 'var(--gw-accent)',
    opacity: '0.28',
  },
  '.cm-content ::selection': { backgroundColor: 'transparent' },
  // Blink needs to be obvious to find the caret in a wall of text.
  '.cm-cursorLayer': { animation: 'cm-blink 1.1s steps(1) infinite' },
  '@keyframes cm-blink': { '0%, 49%': { opacity: '1' }, '50%, 100%': { opacity: '0.15' } },
})

export function SpecEditor({
  value,
  onChange,
  onSave,
  ariaLabel,
}: {
  value: string
  onChange: (next: string) => void
  /** Ctrl/Cmd+S. The button is the discoverable path; this is for the habit. */
  onSave?: () => void
  ariaLabel: string
}) {
  const host = useRef<HTMLDivElement>(null)
  const view = useRef<EditorView | null>(null)
  // Held in a ref so the editor is built once. Rebuilding it on every keystroke
  // to capture a fresh closure would drop the cursor and the undo history.
  const handlers = useRef({ onChange, onSave })
  handlers.current = { onChange, onSave }
  /**
   * The document to build with, kept current for the build effect below.
   *
   * The effect runs with `[]` deps, so its closure captures the `value` from the
   * first render and never sees a later one. That is fine when the editor is
   * built once -- but StrictMode mounts effects twice (run, clean up, run
   * again), and React Fast Refresh does the same. The second build would then
   * re-create the editor from the *first* render's value, which is the empty
   * string when the file is still being read. Reading through a ref makes every
   * build use the text that is current at the moment it runs.
   */
  const initialDoc = useRef(value)
  initialDoc.current = value

  useEffect(() => {
    if (!host.current) return

    const state = EditorState.create({
      doc: initialDoc.current,
      extensions: [
        lineNumbers(),
        history(),
        highlightActiveLine(),
        markdown(),
        syntaxHighlighting(highlight),
        EditorView.lineWrapping,
        theme,
        keymap.of([
          {
            key: 'Mod-s',
            run: () => {
              handlers.current.onSave?.()
              // Claimed even without a handler, so the browser's own Save
              // dialog never opens over the app.
              return true
            },
          },
          ...defaultKeymap,
          ...historyKeymap,
          // Tab indents inside the editor but still tabs *out* when the editor
          // is empty of a selection, so the keyboard is never trapped here.
          indentWithTab,
        ]),
        EditorView.updateListener.of((update) => {
          if (update.docChanged) handlers.current.onChange(update.state.doc.toString())
        }),
        EditorView.contentAttributes.of({ 'aria-label': ariaLabel }),
      ],
    })

    const editor = new EditorView({ state, parent: host.current })
    view.current = editor
    editor.focus()
    return () => {
      editor.destroy()
      view.current = null
    }
    // Built once per mount. `value` is the initial document only; later changes
    // are reconciled by the effect below.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  /**
   * Adopt a `value` that changed outside the editor -- an accepted AI draft
   * replacing what is on screen.
   *
   * Guarded by a comparison against the current document: without it, the
   * round trip from `onChange` back into `value` would dispatch a redundant
   * transaction on every keystroke, fighting the cursor.
   */
  useEffect(() => {
    const editor = view.current
    if (!editor) return
    const current = editor.state.doc.toString()
    if (current === value) return
    editor.dispatch({
      changes: { from: 0, to: current.length, insert: value },
      // Keep the cursor in range rather than wherever it was in the old text.
      selection: { anchor: Math.min(editor.state.selection.main.anchor, value.length) },
    })
  }, [value])

  return <div ref={host} className="h-full min-h-0 overflow-hidden" />
}
