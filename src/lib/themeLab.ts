// Dev-only theme lab. Live-previews candidate tweakcn themes by overriding the
// --gw-* custom properties on :root at runtime. Every downstream layer
// (shadcn semantic vars -> Tailwind @theme -> utility classes) references those
// vars, so setting them re-themes the whole app instantly. Nothing here is
// persisted -- closing the lab / clearing the override reverts to index.css.
//
// This file ships behind an import.meta.env.DEV gate (see ThemeLabLauncher) and
// is intended to be deleted once a theme is chosen and baked into index.css.

// The full set of surface/text/border tokens the lab drives. Accent tokens are
// handled separately so the mint override can swap only those.
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
  id: string
  name: string
  /** One-line description of the theme's native character. */
  note: string
  /** True when the theme's own primary is already in the mint/teal family. */
  nativeMint: boolean
  surface: SurfaceTokens
  accent: AccentTokens
}

// The committed GitWyrm accent (Deep Mint), used as the default for the mint
// override and as the starting value of the custom-mint picker.
export const DEEP_MINT = '#1db584'

// --- oklch -> hex ----------------------------------------------------------
// Small self-contained converter so the lab has no runtime deps. Handles the
// oklch(L C H) form tweakcn exports (L in 0..1, H in degrees).

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

/** Parse an `oklch(L C H)` string and return hex. */
export function oklch(str: string): string {
  const m = str.match(/oklch\(\s*([\d.]+)\s+([\d.]+)\s+([\d.]+)/i)
  if (!m) return str
  return oklchToHex(parseFloat(m[1]), parseFloat(m[2]), parseFloat(m[3]))
}

// --- accent derivation -----------------------------------------------------
// Given a base mint hex, derive the four accent tokens the app uses:
//   accent      -> the fill (buttons, badges, borders)
//   accentText  -> a slightly lighter value for accent-colored text
//   accentFg    -> near-black text that sits ON the accent fill
//   accentSoft  -> a translucent wash for hover/selected backgrounds

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

/** Lighten a hex toward white by `amt` (0..1). */
function lighten(hex: string, amt: number): string {
  const [r, g, b] = hexToRgb(hex)
  return rgbToHex(r + (255 - r) * amt, g + (255 - g) * amt, b + (255 - b) * amt)
}

export function deriveAccent(base: string): AccentTokens {
  const [r, g, b] = hexToRgb(base)
  return {
    accent: base,
    accentText: lighten(base, 0.12),
    accentFg: '#04120d',
    accentSoft: `rgba(${r}, ${g}, ${b}, 0.13)`,
  }
}

// --- the candidate themes --------------------------------------------------
// Values pulled from tweakcn registries (dark mode). Surfaces converted from
// oklch to hex; accents likewise. `nativeMint` flags themes whose own primary
// already lives in the mint/teal family.

function surface(vals: {
  bg: string
  panel: string
  panel2: string
  panel3: string
  border: string
  modal: string
  text: string
  sub: string
  muted: string
}): SurfaceTokens {
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

export const THEMES: ThemeDef[] = [
  {
    id: 'gitwyrm',
    name: 'GitWyrm (current)',
    note: 'Your committed blue-slate + Deep Mint.',
    nativeMint: true,
    surface: {
      bg: '#0b0f14',
      panel: '#0f151c',
      panel2: '#131b24',
      panel3: '#1a2430',
      border: '#1e2833',
      modal: '#151d26',
      text: '#d7e0ea',
      sub: '#8b99a8',
      muted: '#5d6b7a',
    },
    accent: deriveAccent('#1db584'),
  },
  {
    id: 'northern-lights',
    name: 'Northern Lights',
    note: 'Green primary, faintly blue background, blue secondary accent.',
    nativeMint: true,
    surface: surface({
      bg: 'oklch(0.2303 0.0125 264.2926)',
      panel: 'oklch(0.3210 0.0078 223.6661)',
      panel2: 'oklch(0.2800 0.0100 250)',
      panel3: 'oklch(0.3600 0.0100 250)',
      border: 'oklch(0.3600 0.0100 250)',
      modal: 'oklch(0.3210 0.0078 223.6661)',
      text: 'oklch(0.9200 0 0)',
      sub: 'oklch(0.7000 0.0100 250)',
      muted: 'oklch(0.5500 0.0100 250)',
    }),
    accent: deriveAccent(oklch('oklch(0.6487 0.1538 150.3071)')),
  },
  {
    id: 'astrovista',
    name: 'AstroVista',
    note: 'Pure neutral gray surfaces (nicely lifted). Native accent is orange.',
    nativeMint: false,
    surface: surface({
      bg: 'oklch(0.2178 0 0)',
      panel: 'oklch(0.2435 0 0)',
      panel2: 'oklch(0.2850 0 0)',
      panel3: 'oklch(0.3290 0 0)',
      border: 'oklch(0.3290 0 0)',
      modal: 'oklch(0.2435 0 0)',
      text: 'oklch(0.9219 0 0)',
      sub: 'oklch(0.7000 0 0)',
      muted: 'oklch(0.5500 0 0)',
    }),
    accent: deriveAccent(oklch('oklch(0.6420 0.1691 38.5815)')),
  },
  {
    id: 'cloudflare',
    name: 'Cloudflare',
    note: 'Pure neutral gray, slightly darker. Native accent is orange.',
    nativeMint: false,
    surface: surface({
      bg: 'oklch(0.1822 0 0)',
      panel: 'oklch(0.2178 0 0)',
      panel2: 'oklch(0.2686 0 0)',
      panel3: 'oklch(0.2972 0 0)',
      border: 'oklch(0.2972 0 0)',
      modal: 'oklch(0.2178 0 0)',
      text: 'oklch(0.9219 0 0)',
      sub: 'oklch(0.7000 0 0)',
      muted: 'oklch(0.5500 0 0)',
    }),
    accent: deriveAccent(oklch('oklch(0.6848 0.1832 41.8733)')),
  },
  {
    id: 'aie',
    name: 'AIE',
    note: 'Neutral gray, evenly stepped panels, faint cool border only.',
    nativeMint: false,
    surface: surface({
      // Base pulled below the original card color so panels sit ABOVE bg with
      // real contrast; panels stay neutral, only the border carries the cool
      // tint AIE is known for.
      bg: 'oklch(0.1750 0 0)',
      panel: 'oklch(0.2090 0 0)',
      panel2: 'oklch(0.2450 0 0)',
      panel3: 'oklch(0.2850 0 0)',
      border: 'oklch(0.3200 0.0110 215)',
      modal: 'oklch(0.2280 0 0)',
      text: 'oklch(0.9200 0 0)',
      sub: 'oklch(0.6800 0 0)',
      muted: 'oklch(0.5200 0 0)',
    }),
    accent: deriveAccent(oklch('oklch(0.6138 0.1889 24.6219)')),
  },
  {
    id: 'claude-plus',
    name: 'Claude +',
    note: 'Warm charcoal base with real panel separation. Native accent is clay.',
    nativeMint: false,
    surface: surface({
      // Original ramp started at the theme's light CARD color (L0.27) and only
      // went lighter, so the whole app washed out. Anchor a genuinely dark warm
      // base and step the panels up from there, keeping the warm hue ~106.
      bg: 'oklch(0.1900 0.0040 106)',
      panel: 'oklch(0.2250 0.0045 106)',
      panel2: 'oklch(0.2620 0.0050 106)',
      panel3: 'oklch(0.3050 0.0060 106)',
      border: 'oklch(0.3300 0.0080 106)',
      modal: 'oklch(0.2450 0.0050 106)',
      text: 'oklch(0.9300 0.0050 95)',
      sub: 'oklch(0.6900 0.0050 95)',
      muted: 'oklch(0.5300 0.0050 95)',
    }),
    accent: deriveAccent(oklch('oklch(0.6724 0.1308 38.7559)')),
  },
  {
    id: 'type-zed',
    name: 'Type zed',
    note: 'Deep blue primary on faint-blue neutral. Cool and techy.',
    nativeMint: false,
    surface: surface({
      bg: 'oklch(0.1964 0.0082 274.4865)',
      panel: 'oklch(0.2404 0.0096 276.6737)',
      panel2: 'oklch(0.2686 0.0097 268.3127)',
      panel3: 'oklch(0.2936 0.0112 271.0138)',
      border: 'oklch(0.2936 0.0112 271.0138)',
      modal: 'oklch(0.2404 0.0096 276.6737)',
      text: 'oklch(0.9789 0.0013 106.4235)',
      sub: 'oklch(0.7200 0.0090 271)',
      muted: 'oklch(0.5600 0.0090 271)',
    }),
    accent: deriveAccent(oklch('oklch(0.4793 0.2304 264.0187)')),
  },
]

// --- the apply engine ------------------------------------------------------
// Every property the lab drives, mapped to its CSS custom property name.

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

export type ThemePreview = {
  surface: SurfaceTokens
  accent: AccentTokens
}

/** Resolve the tokens a theme should apply, given the mint-override choice. */
export function resolvePreview(
  theme: ThemeDef,
  mintOverride: boolean,
  customMint: string | null,
): ThemePreview {
  let accent = theme.accent
  if (mintOverride) {
    accent = deriveAccent(customMint ?? DEEP_MINT)
  }
  return { surface: theme.surface, accent }
}

/** Write the preview onto :root. Reversible via clearThemeOverride. */
export function applyThemeOverride(preview: ThemePreview): void {
  const root = document.documentElement
  ;(Object.keys(SURFACE_VARS) as (keyof SurfaceTokens)[]).forEach((k) => {
    root.style.setProperty(SURFACE_VARS[k], preview.surface[k])
  })
  ;(Object.keys(ACCENT_VARS) as (keyof AccentTokens)[]).forEach((k) => {
    root.style.setProperty(ACCENT_VARS[k], preview.accent[k])
  })
}

/** Remove all lab-set inline properties, reverting to index.css. */
export function clearThemeOverride(): void {
  const root = document.documentElement
  Object.values(SURFACE_VARS).forEach((v) => root.style.removeProperty(v))
  Object.values(ACCENT_VARS).forEach((v) => root.style.removeProperty(v))
}

// --- cross-window event contract -------------------------------------------
// The switcher (popout window) emits these; the main window listens and applies
// them. Payload is the resolved preview, or null to clear.

export const THEME_PREVIEW_EVENT = 'theme-lab://preview'
export type ThemePreviewPayload = ThemePreview | null
