// Dev-only theme switcher. Renders as the entire content of the theme-lab
// popout window. Every interaction emits a `theme-lab://preview` Tauri event
// that the main window listens for and applies live. Nothing is persisted.
//
// Delete this file (and themeLab.ts, ThemeLabLauncher.tsx) once a theme is
// chosen and baked into index.css.
import { useCallback, useEffect, useMemo, useState } from 'react'
import { emit } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import {
  THEMES,
  DEEP_MINT,
  resolvePreview,
  applyThemeOverride,
  clearThemeOverride,
  THEME_PREVIEW_EVENT,
  type ThemeDef,
} from '@/lib/themeLab'
import { cn } from '@/lib/utils'

export function ThemeLab() {
  const [selectedId, setSelectedId] = useState<string>(THEMES[0].id)
  const [mintOverride, setMintOverride] = useState(false)
  const [customMint, setCustomMint] = useState<string>(DEEP_MINT)
  const [useCustom, setUseCustom] = useState(false)

  const selected = useMemo(
    () => THEMES.find((t) => t.id === selectedId) ?? THEMES[0],
    [selectedId],
  )

  // The single source of truth for what's on screen. Recomputed whenever any
  // control changes, then both applied locally (so the popout previews too) and
  // broadcast to the main window.
  const broadcast = useCallback(
    (theme: ThemeDef, mint: boolean, custom: string | null) => {
      const preview = resolvePreview(theme, mint, custom)
      applyThemeOverride(preview)
      void emit(THEME_PREVIEW_EVENT, preview)
    },
    [],
  )

  useEffect(() => {
    broadcast(selected, mintOverride, useCustom ? customMint : null)
  }, [broadcast, selected, mintOverride, useCustom, customMint])

  // Clearing must reach the main window too, so route it through an empty emit.
  const reset = useCallback(() => {
    clearThemeOverride()
    void emit(THEME_PREVIEW_EVENT, null)
  }, [])

  // On close, revert the main window to its committed theme -- a live preview
  // should never outlive the lab.
  useEffect(() => {
    const win = getCurrentWindow()
    const unlisten = win.onCloseRequested(() => {
      void emit(THEME_PREVIEW_EVENT, null)
    })
    return () => {
      void unlisten.then((f) => f())
    }
  }, [])

  const effectiveAccent = useMemo(() => {
    if (mintOverride) return useCustom ? customMint : DEEP_MINT
    return selected.accent.accent
  }, [mintOverride, useCustom, customMint, selected])

  return (
    <div className="flex h-screen flex-col bg-background text-foreground">
      {/* Draggable title strip -- decorations are off on this window too. */}
      <div
        data-tauri-drag-region
        className="flex items-center justify-between border-b border-border px-4 py-2"
      >
        <span className="text-sm font-medium">Theme Lab</span>
        <button
          onClick={() => getCurrentWindow().close()}
          className="titlebar-no-drag rounded px-2 py-0.5 text-xs text-sub hover:bg-panel3 hover:text-foreground"
        >
          Close
        </button>
      </div>

      <div className="flex-1 overflow-y-auto p-4">
        {/* Mint override controls */}
        <section className="mb-4 rounded-lg border border-border bg-panel p-3">
          <label className="flex cursor-pointer items-center justify-between">
            <div>
              <div className="text-sm font-medium">Force mint accent</div>
              <div className="text-2xs text-sub">
                Replace each theme's own accent with your mint.
              </div>
            </div>
            <input
              type="checkbox"
              checked={mintOverride}
              onChange={(e) => setMintOverride(e.target.checked)}
              className="size-4 accent-[var(--gw-accent)]"
            />
          </label>

          {mintOverride && (
            <div className="mt-3 border-t border-border pt-3">
              <label className="flex cursor-pointer items-center justify-between">
                <div className="text-sm">Custom mint</div>
                <input
                  type="checkbox"
                  checked={useCustom}
                  onChange={(e) => setUseCustom(e.target.checked)}
                  className="size-4 accent-[var(--gw-accent)]"
                />
              </label>
              {useCustom && (
                <div className="mt-2 flex items-center gap-2">
                  <input
                    type="color"
                    value={customMint}
                    onChange={(e) => setCustomMint(e.target.value)}
                    className="h-8 w-12 cursor-pointer rounded border border-border bg-transparent"
                  />
                  <input
                    type="text"
                    value={customMint}
                    onChange={(e) => setCustomMint(e.target.value)}
                    className="h-8 flex-1 rounded border border-border bg-background px-2 font-mono text-xs"
                  />
                  <button
                    onClick={() => setCustomMint(DEEP_MINT)}
                    className="h-8 rounded border border-border px-2 text-2xs text-sub hover:text-foreground"
                  >
                    Reset
                  </button>
                </div>
              )}
            </div>
          )}
        </section>

        {/* Live accent swatch */}
        <div className="mb-4 flex items-center gap-2 text-2xs text-sub">
          <span>Active accent</span>
          <span
            className="inline-block size-4 rounded-full border border-border"
            style={{ background: effectiveAccent }}
          />
          <span className="font-mono">{effectiveAccent}</span>
        </div>

        {/* Theme list */}
        <div className="space-y-1.5">
          {THEMES.map((theme) => {
            const active = theme.id === selectedId
            return (
              <button
                key={theme.id}
                onClick={() => setSelectedId(theme.id)}
                className={cn(
                  'flex w-full items-center gap-3 rounded-lg border p-2.5 text-left transition-colors',
                  active
                    ? 'border-[var(--gw-accent)] bg-[var(--gw-accent-soft)]'
                    : 'border-border bg-panel hover:bg-panel2',
                )}
              >
                {/* Surface preview: three swatches + accent dot */}
                <div className="flex shrink-0 items-center">
                  <span
                    className="size-6 rounded-l border border-border"
                    style={{ background: theme.surface.bg }}
                  />
                  <span
                    className="size-6 border-y border-border"
                    style={{ background: theme.surface.panel2 }}
                  />
                  <span
                    className="flex size-6 items-center justify-center rounded-r border border-border"
                    style={{ background: theme.surface.panel3 }}
                  >
                    <span
                      className="size-2.5 rounded-full"
                      style={{ background: theme.accent.accent }}
                    />
                  </span>
                </div>
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-1.5">
                    <span className="truncate text-sm font-medium">
                      {theme.name}
                    </span>
                    {theme.nativeMint && (
                      <span className="rounded bg-[var(--gw-accent-soft)] px-1 text-[10px] text-accent-text">
                        mint
                      </span>
                    )}
                  </div>
                  <div className="truncate text-2xs text-sub">{theme.note}</div>
                </div>
              </button>
            )
          })}
        </div>
      </div>

      <div className="flex items-center justify-between border-t border-border px-4 py-2">
        <button
          onClick={reset}
          className="rounded px-2 py-1 text-xs text-sub hover:text-foreground"
        >
          Reset to committed theme
        </button>
        <span className="text-2xs text-muted">Live preview -- nothing saved</span>
      </div>
    </div>
  )
}
