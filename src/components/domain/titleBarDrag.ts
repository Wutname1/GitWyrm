import { getCurrentWindow } from "@tauri-apps/api/window";
import type { MouseEvent } from "react";

/**
 * Double-clicking a title bar maximizes or restores the window everywhere else
 * on the desktop, so the app bar has to do it too. The check keeps the handler
 * off buttons and tabs inside the bar, which have their own double-click
 * behavior (renaming a tab, for one).
 */
export function onTitleBarDoubleClick(event: MouseEvent<HTMLElement>) {
  if (!(event.target as HTMLElement).hasAttribute("data-tauri-drag-region"))
    return;
  void getCurrentWindow()
    .toggleMaximize()
    .catch(() => {});
}
