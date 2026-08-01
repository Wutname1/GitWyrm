import { useEffect } from 'react'
import { useWorkspaceStore } from '@/stores/workspaceStore'

/**
 * Applies the user's zoom to the body, not #root. `zoom` scales layout and every
 * pixel value (unlike a font-size trick), which is what we want for a git client
 * full of fixed-size rows and badges.
 *
 * It has to land on the body because dialogs, popovers, dropdowns and tooltips
 * portal to document.body, which sits outside #root. Zooming #root alone leaves
 * every overlay stuck at 100% while the app behind it scales.
 *
 * Everything below is sized in percentages, not viewport units: percentages
 * resolve against the already-zoomed containing block, so the layout still fits
 * the window exactly at any scale. Viewport units (dvh/vw) do not shrink under
 * zoom, which overflows the window and pushes the status bar -- and the zoom
 * control itself -- off-screen with no way back.
 *
 * Mount once per window, alongside useTheme and useFont: every window the user
 * can see is expected to honour the same zoom.
 */
export function useUiScale() {
  const uiScale = useWorkspaceStore((s) => s.uiScale)

  useEffect(() => {
    document.body.style.zoom = String(uiScale)
  }, [uiScale])
}
