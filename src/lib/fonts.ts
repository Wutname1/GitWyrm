// UI font selection. The app's sans-serif text is driven by the --gw-font-sans
// custom property (see index.css); this module lists the fonts the user can
// choose and, on Windows/WebView2, enumerates the fonts installed on the machine
// via the Local Font Access API.

/** A selectable UI font. `stack` is the full CSS font-family value to apply. */
export interface FontOption {
  /** Stored id. For system fonts this is the family name itself. */
  id: string
  /** Label shown in the picker. */
  name: string
  /** CSS font-family stack applied to --gw-font-sans. */
  stack: string
  /** Bundled fonts ship with the app and always render; system fonts may vary. */
  bundled: boolean
}

/** The default font id (IBM Plex Sans, the app's original UI font). */
export const DEFAULT_FONT_ID = 'plex'

/**
 * Fonts bundled with the app (imported in index.css). These always render
 * regardless of what is installed on the machine, so they lead the list.
 */
export const BUNDLED_FONTS: FontOption[] = [
  {
    id: 'plex',
    name: 'IBM Plex Sans',
    stack: '"IBM Plex Sans Variable", system-ui, sans-serif',
    bundled: true,
  },
  {
    id: 'inter',
    name: 'Inter',
    stack: '"Inter Variable", system-ui, sans-serif',
    bundled: true,
  },
  {
    id: 'geist',
    name: 'Geist',
    stack: '"Geist Variable", system-ui, sans-serif',
    bundled: true,
  },
  {
    id: 'roboto',
    name: 'Roboto',
    stack: '"Roboto Variable", system-ui, sans-serif',
    bundled: true,
  },
  {
    id: 'system',
    name: 'System default',
    stack: 'system-ui, "Segoe UI", sans-serif',
    bundled: true,
  },
]

/** Build a system-font option from an installed family name. */
function systemFont(family: string): FontOption {
  return { id: family, name: family, stack: `"${family}", system-ui, sans-serif`, bundled: false }
}

/** Look up a font by id across bundled and system fonts. */
export function findFont(id: string, systemFonts: FontOption[]): FontOption | undefined {
  return (
    BUNDLED_FONTS.find((f) => f.id === id) ??
    systemFonts.find((f) => f.id === id) ??
    // A saved system font that is no longer enumerated: still apply it by name.
    (id && id !== DEFAULT_FONT_ID ? systemFont(id) : undefined)
  )
}

/** Resolve the CSS stack for a saved font id. Falls back to the default font. */
export function fontStack(id: string, systemFonts: FontOption[]): string {
  return (findFont(id, systemFonts) ?? BUNDLED_FONTS[0]).stack
}

interface LocalFontData {
  family: string
}

interface QueryLocalFontsWindow {
  queryLocalFonts?: () => Promise<LocalFontData[]>
}

/** True when this runtime exposes the Local Font Access API (WebView2 on Windows does). */
export function canQuerySystemFonts(): boolean {
  return typeof (window as QueryLocalFontsWindow).queryLocalFonts === 'function'
}

/**
 * Enumerate the sans-appropriate fonts installed on the machine, de-duplicated
 * by family and sorted. Returns [] when the API is unavailable or the user
 * declines the permission prompt.
 */
export async function loadSystemFonts(): Promise<FontOption[]> {
  const query = (window as QueryLocalFontsWindow).queryLocalFonts
  if (!query) return []
  try {
    const fonts = await query()
    const families = [...new Set(fonts.map((f) => f.family))]
      .filter((family) => family.trim().length > 0)
      .sort((a, b) => a.localeCompare(b))
    return families.map(systemFont)
  } catch {
    // Permission denied or the API threw: fall back to bundled fonts only.
    return []
  }
}
