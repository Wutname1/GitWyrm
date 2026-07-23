import { useEffect } from 'react'
import { useWorkspaceStore } from '@/stores/workspaceStore'
import { applyTokens, resolveTokens } from '@/lib/themes'

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
    const media = window.matchMedia('(prefers-color-scheme: dark)')

    const apply = () => {
      applyTokens(resolveTokens(theme, themeMode, mintAccent, media.matches))
    }
    apply()

    // Only the 'system' mode depends on the OS preference; still safe to listen
    // always -- apply() re-reads media.matches and is a no-op for fixed modes.
    media.addEventListener('change', apply)
    return () => media.removeEventListener('change', apply)
  }, [theme, themeMode, mintAccent])
}
