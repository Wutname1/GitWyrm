// Color themes. Each theme carries a dark and a light variant; the active
// tokens are written onto :root as --gw-* custom properties, which the whole
// token chain (shadcn semantic vars -> Tailwind @theme -> utilities) reads.
//
// Applying a theme is just overriding those custom properties at runtime; see
// useTheme.ts for the hook that resolves + applies the active theme.

export type Mode = 'dark' | 'light'

/** Light/dark preference. 'system' follows the OS. */
export type ThemeMode = 'light' | 'dark' | 'system'

/** Theme selection. 'auto' picks Slate in dark and Paper in light. */
export type ThemeId = 'auto' | 'slate' | 'onyx' | 'midnight' | 'paper'

export const THEME_IDS: ThemeId[] = ['auto', 'slate', 'onyx', 'midnight', 'paper']
export const THEME_MODES: ThemeMode[] = ['system', 'light', 'dark']

// The theme each Auto mode falls back to.
const AUTO_DARK: Exclude<ThemeId, 'auto'> = 'slate'
const AUTO_LIGHT: Exclude<ThemeId, 'auto'> = 'paper'

// The GitWyrm mint accent (on by default; toggle in settings to reveal each
// theme's native accent instead).
export const DEEP_MINT = '#1db584'

export type SurfaceTokens = {
  bg: string
  panel: string
  panel2: string
  panel3: string
  border: string
  modal: string
  text: string
  sub: string
  muted: string
}

export type AccentTokens = {
  accent: string
  accentText: string
  accentFg: string
  accentSoft: string
}

export type ThemeDef = {
  id: Exclude<ThemeId, 'auto'>
  name: string
  /** One-line, user-facing description for the settings UI. */
  note: string
  /** Which modes this theme is designed for (drives the settings picker). */
  suits: Mode
  dark: { surface: SurfaceTokens; accent: AccentTokens }
  light: { surface: SurfaceTokens; accent: AccentTokens }
}

// --- oklch -> hex ----------------------------------------------------------

function oklchToHex(L: number, C: number, H: number): string {
  const hr = (H * Math.PI) / 180
  const a = Math.cos(hr) * C
  const b = Math.sin(hr) * C
  const l_ = L + 0.3963377774 * a + 0.2158037573 * b
  const m_ = L - 0.1055613458 * a - 0.0638541728 * b
  const s_ = L - 0.0894841775 * a - 1.291485548 * b
  const l = l_ * l_ * l_
  const m = m_ * m_ * m_
  const s = s_ * s_ * s_
  let r = +4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s
  let g = -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s
  let bl = -0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s
  const toSrgb = (x: number) => {
    const v = x <= 0.0031308 ? 12.92 * x : 1.055 * Math.pow(x, 1 / 2.4) - 0.055
    return Math.max(0, Math.min(1, v))
  }
  r = toSrgb(r)
  g = toSrgb(g)
  bl = toSrgb(bl)
  const hex = (x: number) =>
    Math.round(x * 255)
      .toString(16)
      .padStart(2, '0')
  return `#${hex(r)}${hex(g)}${hex(bl)}`
}

function oklch(str: string): string {
  const m = str.match(/oklch\(\s*([\d.]+)\s+([\d.]+)\s+([\d.]+)/i)
  if (!m) return str
  return oklchToHex(parseFloat(m[1]), parseFloat(m[2]), parseFloat(m[3]))
}

// --- accent derivation -----------------------------------------------------

function hexToRgb(hex: string): [number, number, number] {
  const h = hex.replace('#', '')
  return [
    parseInt(h.slice(0, 2), 16),
    parseInt(h.slice(2, 4), 16),
    parseInt(h.slice(4, 6), 16),
  ]
}

function rgbToHex(r: number, g: number, b: number): string {
  const c = (x: number) =>
    Math.max(0, Math.min(255, Math.round(x)))
      .toString(16)
      .padStart(2, '0')
  return `#${c(r)}${c(g)}${c(b)}`
}

function lighten(hex: string, amt: number): string {
  const [r, g, b] = hexToRgb(hex)
  return rgbToHex(r + (255 - r) * amt, g + (255 - g) * amt, b + (255 - b) * amt)
}

function darken(hex: string, amt: number): string {
  const [r, g, b] = hexToRgb(hex)
  return rgbToHex(r * (1 - amt), g * (1 - amt), b * (1 - amt))
}

function luminance(hex: string): number {
  const [r, g, b] = hexToRgb(hex)
  return (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255
}

/**
 * Derive the four accent tokens for a mode.
 * - accentFg (text ON the fill) flips to near-black/white by luminance so a
 *   pale accent keeps readable text.
 * - accentText (accent-colored text on a surface) lightens in dark, darkens in
 *   light, so it stays legible against the background.
 */
export function deriveAccent(base: string, mode: Mode): AccentTokens {
  const [r, g, b] = hexToRgb(base)
  const fg = luminance(base) > 0.55 ? '#0a0a0a' : '#04120d'
  return {
    accent: base,
    accentText: mode === 'light' ? darken(base, 0.28) : lighten(base, 0.12),
    accentFg: fg,
    accentSoft: `rgba(${r}, ${g}, ${b}, ${mode === 'light' ? 0.16 : 0.13})`,
  }
}

// --- theme data ------------------------------------------------------------

function surface(vals: SurfaceTokens): SurfaceTokens {
  return {
    bg: oklch(vals.bg),
    panel: oklch(vals.panel),
    panel2: oklch(vals.panel2),
    panel3: oklch(vals.panel3),
    border: oklch(vals.border),
    modal: oklch(vals.modal),
    text: oklch(vals.text),
    sub: oklch(vals.sub),
    muted: oklch(vals.muted),
  }
}

function modeEntry(surfaceVals: SurfaceTokens, accentHex: string, mode: Mode) {
  return { surface: surface(surfaceVals), accent: deriveAccent(accentHex, mode) }
}

export const THEMES: ThemeDef[] = [
  {
    id: 'slate',
    name: 'Slate',
    note: 'Clean neutral gray. The default dark theme.',
    suits: 'dark',
    dark: modeEntry(
      {
        bg: 'oklch(0.1822 0 0)',
        panel: 'oklch(0.2178 0 0)',
        panel2: 'oklch(0.2686 0 0)',
        panel3: 'oklch(0.2972 0 0)',
        border: 'oklch(0.2972 0 0)',
        modal: 'oklch(0.2178 0 0)',
        text: 'oklch(0.9219 0 0)',
        sub: 'oklch(0.7000 0 0)',
        muted: 'oklch(0.5500 0 0)',
      },
      oklch('oklch(0.6848 0.1832 41.8733)'),
      'dark',
    ),
    light: modeEntry(
      {
        bg: 'oklch(0.9851 0 0)',
        panel: 'oklch(1.0000 0 0)',
        panel2: 'oklch(0.9672 0 0)',
        panel3: 'oklch(0.9400 0 0)',
        border: 'oklch(0.9219 0 0)',
        modal: 'oklch(1.0000 0 0)',
        text: 'oklch(0.1908 0.0020 106.5859)',
        sub: 'oklch(0.4200 0 0)',
        muted: 'oklch(0.5103 0 0)',
      },
      oklch('oklch(0.6848 0.1832 41.8733)'),
      'light',
    ),
  },
  {
    id: 'onyx',
    name: 'Onyx',
    note: 'The darkest theme, with a faint cool edge on panels.',
    suits: 'dark',
    dark: modeEntry(
      {
        bg: 'oklch(0.1750 0 0)',
        panel: 'oklch(0.2090 0 0)',
        panel2: 'oklch(0.2450 0 0)',
        panel3: 'oklch(0.2850 0 0)',
        border: 'oklch(0.3200 0.0110 215)',
        modal: 'oklch(0.2280 0 0)',
        text: 'oklch(0.9200 0 0)',
        sub: 'oklch(0.6800 0 0)',
        muted: 'oklch(0.5200 0 0)',
      },
      oklch('oklch(0.6138 0.1889 24.6219)'),
      'dark',
    ),
    light: modeEntry(
      {
        bg: 'oklch(0.9500 0 0)',
        panel: 'oklch(1.0000 0 0)',
        panel2: 'oklch(0.9700 0 0)',
        panel3: 'oklch(0.9400 0 0)',
        border: 'oklch(0.8700 0.0090 215)',
        modal: 'oklch(1.0000 0 0)',
        text: 'oklch(0.2200 0 0)',
        sub: 'oklch(0.4600 0 0)',
        muted: 'oklch(0.5600 0 0)',
      },
      oklch('oklch(0.6138 0.1889 24.6219)'),
      'light',
    ),
  },
  {
    id: 'midnight',
    name: 'Midnight',
    note: 'Cool blue-tinted surfaces for a calmer, deeper look.',
    suits: 'dark',
    dark: modeEntry(
      {
        bg: 'oklch(0.1964 0.0082 274.4865)',
        panel: 'oklch(0.2404 0.0096 276.6737)',
        panel2: 'oklch(0.2686 0.0097 268.3127)',
        panel3: 'oklch(0.2936 0.0112 271.0138)',
        border: 'oklch(0.2936 0.0112 271.0138)',
        modal: 'oklch(0.2404 0.0096 276.6737)',
        text: 'oklch(0.9789 0.0013 106.4235)',
        sub: 'oklch(0.7200 0.0090 271)',
        muted: 'oklch(0.5600 0.0090 271)',
      },
      oklch('oklch(0.4793 0.2304 264.0187)'),
      'dark',
    ),
    light: modeEntry(
      {
        bg: 'oklch(0.9878 0.0013 106.4233)',
        panel: 'oklch(1.0000 0 0)',
        panel2: 'oklch(0.9549 0.0013 106.4243)',
        panel3: 'oklch(0.9244 0.0027 106.4517)',
        border: 'oklch(0.9183 0.0027 106.4521)',
        modal: 'oklch(1.0000 0 0)',
        text: 'oklch(0.2257 0.0095 97.7753)',
        sub: 'oklch(0.4400 0.0080 89)',
        muted: 'oklch(0.5454 0.0080 88.6849)',
      },
      oklch('oklch(0.4902 0.2546 263.8475)'),
      'light',
    ),
  },
  {
    id: 'paper',
    name: 'Paper',
    note: 'Bright neutral surfaces. The default light theme.',
    suits: 'light',
    dark: modeEntry(
      {
        bg: 'oklch(0.2178 0 0)',
        panel: 'oklch(0.2435 0 0)',
        panel2: 'oklch(0.2850 0 0)',
        panel3: 'oklch(0.3290 0 0)',
        border: 'oklch(0.3290 0 0)',
        modal: 'oklch(0.2435 0 0)',
        text: 'oklch(0.9219 0 0)',
        sub: 'oklch(0.7000 0 0)',
        muted: 'oklch(0.5500 0 0)',
      },
      oklch('oklch(0.6420 0.1691 38.5815)'),
      'dark',
    ),
    light: modeEntry(
      {
        bg: 'oklch(0.9383 0.0042 236.4993)',
        panel: 'oklch(1.0000 0 0)',
        panel2: 'oklch(0.9650 0.0030 236)',
        panel3: 'oklch(0.9350 0.0040 236)',
        border: 'oklch(0.8452 0 0)',
        modal: 'oklch(1.0000 0 0)',
        text: 'oklch(0.3211 0 0)',
        sub: 'oklch(0.4800 0.0150 264)',
        muted: 'oklch(0.5510 0.0234 264.3637)',
      },
      oklch('oklch(0.6420 0.1691 38.5815)'),
      'light',
    ),
  },
]

export function getTheme(id: Exclude<ThemeId, 'auto'>): ThemeDef {
  return THEMES.find((t) => t.id === id) ?? THEMES[0]
}

// --- resolution ------------------------------------------------------------

/** Resolve ThemeMode + OS preference into a concrete light/dark mode. */
export function resolveMode(mode: ThemeMode, systemPrefersDark: boolean): Mode {
  if (mode === 'system') return systemPrefersDark ? 'dark' : 'light'
  return mode
}

/**
 * Resolve the theme + mode selection into a concrete theme id.
 * 'auto' picks Slate in dark and Paper in light.
 */
export function resolveThemeId(
  theme: ThemeId,
  resolvedMode: Mode,
): Exclude<ThemeId, 'auto'> {
  if (theme === 'auto') return resolvedMode === 'dark' ? AUTO_DARK : AUTO_LIGHT
  return theme
}

export type ResolvedTokens = { surface: SurfaceTokens; accent: AccentTokens }

/** Full resolution: settings -> the exact tokens to write onto :root. */
export function resolveTokens(
  theme: ThemeId,
  mode: ThemeMode,
  mintAccent: boolean,
  systemPrefersDark: boolean,
): ResolvedTokens {
  const m = resolveMode(mode, systemPrefersDark)
  const def = getTheme(resolveThemeId(theme, m))
  const entry = def[m]
  const accent = mintAccent ? deriveAccent(DEEP_MINT, m) : entry.accent
  return { surface: entry.surface, accent }
}

// --- apply engine ----------------------------------------------------------

const SURFACE_VARS: Record<keyof SurfaceTokens, string> = {
  bg: '--gw-bg',
  panel: '--gw-panel',
  panel2: '--gw-panel2',
  panel3: '--gw-panel3',
  border: '--gw-border',
  modal: '--gw-modal',
  text: '--gw-text',
  sub: '--gw-sub',
  muted: '--gw-muted',
}

const ACCENT_VARS: Record<keyof AccentTokens, string> = {
  accent: '--gw-accent',
  accentText: '--gw-accent-text',
  accentFg: '--gw-accent-fg',
  accentSoft: '--gw-accent-soft',
}

/** Write resolved tokens onto :root. */
export function applyTokens(tokens: ResolvedTokens): void {
  const root = document.documentElement
  ;(Object.keys(SURFACE_VARS) as (keyof SurfaceTokens)[]).forEach((k) => {
    root.style.setProperty(SURFACE_VARS[k], tokens.surface[k])
  })
  ;(Object.keys(ACCENT_VARS) as (keyof AccentTokens)[]).forEach((k) => {
    root.style.setProperty(ACCENT_VARS[k], tokens.accent[k])
  })
  // Let the UA theme form controls, scrollbars, etc. to match.
  root.style.colorScheme = luminance(tokens.surface.bg) > 0.5 ? 'light' : 'dark'
}
