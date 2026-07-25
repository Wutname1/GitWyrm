import type { EditorKind } from '@/lib/bindings'
import cursorIcon from '@/assets/icons/cursor.svg'
import jetbrainsIcon from '@/assets/icons/jetbrains.svg'
import vsIcon from '@/assets/icons/vs.svg'
import vscodeIcon from '@/assets/icons/vscode.svg'
import windsurfIcon from '@/assets/icons/windsurf.svg'
import zedIcon from '@/assets/icons/zed.svg'

/**
 * Editor display names and logos.
 *
 * The logos are the brand SVGs in `assets/icons`, rendered as images so they
 * keep the colors and gradients baked into those files -- no re-tinting here.
 */

const editorNames: Record<EditorKind, string> = {
  vs_code: 'VS Code',
  cursor: 'Cursor',
  windsurf: 'Windsurf',
  jetbrains: 'JetBrains',
  zed: 'Zed',
}

/** The editor's name, for buttons, tooltips, and menu entries. */
export function editorLabel(kind: EditorKind): string {
  return editorNames[kind] ?? 'your editor'
}

const editorIcons: Record<EditorKind, string> = {
  vs_code: vscodeIcon,
  cursor: cursorIcon,
  windsurf: windsurfIcon,
  jetbrains: jetbrainsIcon,
  zed: zedIcon,
}

/**
 * Accent used when an editor needs to be represented by a color rather than by
 * its logo -- currently the editor half of the open button's two-tone hint.
 * Each is the dominant color of that editor's logo.
 */
const editorColors: Record<EditorKind, string> = {
  vs_code: '#2196f3',
  cursor: '#e0e0e0',
  windsurf: '#09b6a2',
  jetbrains: '#fe2857',
  zed: '#084ccf',
}

export function editorColor(kind: EditorKind): string {
  return editorColors[kind] ?? editorColors.vs_code
}

/** Visual Studio's purple, for the solution half of the two-tone hint. */
export const VISUAL_STUDIO_COLOR = '#a179dc'

/** An editor's logo, in its own brand colors. */
export function EditorGlyph({
  kind,
  size = 16,
  className,
}: {
  kind: EditorKind
  size?: number
  className?: string
}) {
  return (
    <img
      src={editorIcons[kind] ?? vscodeIcon}
      alt=""
      aria-hidden
      width={size}
      height={size}
      className={className}
      style={{ width: size, height: size }}
    />
  )
}

/** Visual Studio's infinity mark, in its own brand colors. */
export function VisualStudioGlyph({
  size = 12,
  className,
}: {
  size?: number
  className?: string
}) {
  return (
    <img
      src={vsIcon}
      alt=""
      aria-hidden
      width={size}
      height={size}
      className={className}
      style={{ width: size, height: size }}
    />
  )
}

/**
 * The editor's logo with a Visual Studio purple corner dot, marking that the
 * dropdown offers a solution as well as the editor.
 *
 * A dot rather than a diagonal wash over the logo: the brand SVGs carry their
 * own fills and gradients, so recoloring them would fight the artwork and blur
 * which app the icon stands for.
 */
export function EditorWithSolutionGlyph({ kind, size = 16 }: { kind: EditorKind; size?: number }) {
  return (
    <span className="relative flex flex-none" style={{ width: size, height: size }}>
      <EditorGlyph kind={kind} size={size} />
      <span
        className="absolute -bottom-px -right-px rounded-full ring-1 ring-panel2"
        style={{
          width: Math.max(5, Math.round(size * 0.375)),
          height: Math.max(5, Math.round(size * 0.375)),
          backgroundColor: VISUAL_STUDIO_COLOR,
        }}
      />
    </span>
  )
}
