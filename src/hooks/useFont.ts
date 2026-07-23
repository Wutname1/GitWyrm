import { useEffect } from 'react'
import { useWorkspaceStore } from '@/stores/workspaceStore'
import { fontStack } from '@/lib/fonts'

/**
 * Applies the user's font choices to :root, reacting to the family/size/weight
 * settings. The app's --gw-font-* tokens live on :root; this hook overrides them
 * at runtime so every rem-sized label re-flows instantly. Mount once, high in
 * the tree (alongside useTheme).
 *
 * System fonts store their family name as the id, so the stack resolves without
 * needing the enumerated system-font list here.
 */
export function useFont() {
  const fontFamily = useWorkspaceStore((s) => s.fontFamily)
  const fontSize = useWorkspaceStore((s) => s.fontSize)
  const fontWeight = useWorkspaceStore((s) => s.fontWeight)

  useEffect(() => {
    const root = document.documentElement
    root.style.setProperty('--gw-font-sans', fontStack(fontFamily, []))
    root.style.setProperty('--gw-font-size', `${fontSize}rem`)
    root.style.setProperty('--gw-font-weight', String(fontWeight))
  }, [fontFamily, fontSize, fontWeight])
}
