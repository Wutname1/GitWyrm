import { useEffect } from 'react'
import { useWorkspaceStore } from '@/stores/workspaceStore'
import { applyTokens, readSystemScheme, resolveTokens } from '@/lib/themes'

/**
 * Applies the active color theme to :root, reacting to the theme/mode/mint
 * settings and (when mode is 'system') the OS light/dark preference.
 *
 * The app's --gw-* tokens live on :root; this hook overrides them at runtime so
 * the whole token chain (shadcn vars -> Tailwind @theme -> utilities) re-themes
 * instantly. Mount once, high in the tree.
 */
export function useTheme() {
  const theme = useWorkspaceStore((s) => s.theme)
  const themeMode = useWorkspaceStore((s) => s.themeMode)
  const mintAccent = useWorkspaceStore((s) => s.mintAccent)

  useEffect(() => {
    // Both queries are watched: a desktop that gains a preference mid-session
    // (someone flips GNOME to dark) changes the dark query, while one that goes
    // from a preference back to none only changes the light one.
    const dark = window.matchMedia('(prefers-color-scheme: dark)')
    const light = window.matchMedia('(prefers-color-scheme: light)')

    const apply = () => {
      applyTokens(resolveTokens(theme, themeMode, mintAccent, readSystemScheme()))
    }
    apply()

    // Only the 'system' mode depends on the OS preference; still safe to listen
    // always -- apply() re-reads the queries and is a no-op for fixed modes.
    dark.addEventListener('change', apply)
    light.addEventListener('change', apply)
    return () => {
      dark.removeEventListener('change', apply)
      light.removeEventListener('change', apply)
    }
  }, [theme, themeMode, mintAccent])
}
