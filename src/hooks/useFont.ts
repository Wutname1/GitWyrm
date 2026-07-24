import { useEffect } from 'react'
import { DEFAULT_FONT_SIZE, useWorkspaceStore } from '@/stores/workspaceStore'
import { fontStack } from '@/lib/fonts'

/**
 * Applies the user's font choices to :root, reacting to the family/size/weight
 * settings. The app's --gw-font-* tokens live on :root; this hook overrides them
 * at runtime so every rem-sized label re-flows instantly. Mount once, high in
 * the tree (alongside useTheme).
 *
 * The Text Size setting is applied as a scale on the root font-size
 * (--gw-font-scale) rather than a raw size: rem units resolve against the root,
 * so scaling it resizes every label in the app proportionally, and the default
 * setting renders at exactly the designed sizes.
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
    root.style.setProperty('--gw-font-scale', String(fontSize / DEFAULT_FONT_SIZE))
    root.style.setProperty('--gw-font-weight', String(fontWeight))
  }, [fontFamily, fontSize, fontWeight])
}
