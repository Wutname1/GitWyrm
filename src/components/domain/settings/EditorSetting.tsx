import { useQuery } from '@tanstack/react-query'
import { commands, type EditorKind } from '@/lib/bindings'
import { EditorGlyph, editorLabel } from '@/lib/editors'
import { unwrap } from '@/lib/queryKeys'
import { useWorkspaceStore } from '@/stores/workspaceStore'

const selectClass =
  'h-8 w-full rounded-md border border-input bg-background px-2 text-xs text-foreground outline-none focus:border-ring'

/**
 * Picker for the editor the "open in editor" actions launch. Only editors
 * actually found on this computer are offered, so the list can't promise
 * something that would fail on click. A saved choice that is no longer
 * installed stays selected and is marked, rather than silently switching to
 * something the user did not pick.
 */
export function EditorSetting() {
  const defaultEditor = useWorkspaceStore((s) => s.defaultEditor)
  const setDefaultEditor = useWorkspaceStore((s) => s.setDefaultEditor)

  const availability = useQuery({
    queryKey: ['editorAvailability', null],
    queryFn: async () => unwrap(await commands.getEditorAvailability(null)),
    staleTime: 60_000,
  })

  const installed = availability.data?.editors ?? []
  const missing = installed.length > 0 && !installed.some((e) => e.kind === defaultEditor)

  if (availability.isLoading) {
    return <div className="text-xs text-sub">Looking for editors…</div>
  }

  if (installed.length === 0) {
    return (
      <div className="text-xs text-sub">
        No editors found. Install one, then make sure its command-line launcher is on your PATH.
      </div>
    )
  }

  return (
    <div className="space-y-1.5">
      {/* The logo sits beside the select rather than inside it: a native
          <option> cannot render an image. */}
      <div className="flex items-center gap-2">
        <EditorGlyph kind={defaultEditor} size={16} className="flex-none" />
        <select
          className={selectClass}
          value={defaultEditor}
          onChange={(e) => setDefaultEditor(e.target.value as EditorKind)}
        >
          {missing && (
            <option value={defaultEditor}>{editorLabel(defaultEditor)} (not installed)</option>
          )}
          {installed.map((editor) => (
            <option key={editor.kind} value={editor.kind}>
              {editor.label}
            </option>
          ))}
        </select>
      </div>
      {missing && (
        <p className="text-2xs text-sub">
          {editorLabel(defaultEditor)} isn't on this computer any more. Pick another one so the
          open button works.
        </p>
      )}
    </div>
  )
}
