import { getCurrentWindow } from "@tauri-apps/api/window";
import type { MouseEvent as ReactMouseEvent } from "react";

/**
 * Double-clicking a title bar maximizes or restores the window everywhere else
 * on the desktop, so the app bar has to do it too. The check keeps the handler
 * off buttons and tabs inside the bar, which have their own double-click
 * behavior (renaming a tab, for one).
 */
export function onTitleBarDoubleClick(event: ReactMouseEvent<HTMLElement>) {
  if (!(event.target as HTMLElement).hasAttribute("data-tauri-drag-region"))
    return;
  void getCurrentWindow()
    .toggleMaximize()
    .catch(() => {});
}

/**
 * Start the drag ourselves so Linux restores a maximized custom window before
 * moving it. Some X11 window managers do not provide the usual drag-to-restore
 * behavior for Tauri's declarative drag regions.
 */
export function onTitleBarMouseDown(event: ReactMouseEvent<HTMLElement>) {
  if (
    event.button !== 0 ||
    !(event.target as HTMLElement).hasAttribute("data-tauri-drag-region")
  )
    return;

  // Stop Tauri's declarative handler from racing this one, then wait for real
  // pointer movement. Restoring immediately would make a plain click or the
  // first half of a double-click unexpectedly unmaximize the window.
  event.preventDefault();
  event.stopPropagation();
  const startX = event.screenX;
  const startY = event.screenY;

  const stopWatching = () => {
    globalThis.removeEventListener("mousemove", onMove);
    globalThis.removeEventListener("mouseup", stopWatching);
  };
  const onMove = (moveEvent: globalThis.MouseEvent) => {
    if (
      Math.abs(moveEvent.screenX - startX) < 4 &&
      Math.abs(moveEvent.screenY - startY) < 4
    )
      return;

    stopWatching();
    const appWindow = getCurrentWindow();
    void (async () => {
      if (await appWindow.isMaximized()) await appWindow.unmaximize();
      await appWindow.startDragging();
    })().catch(() => {});
  };

  globalThis.addEventListener("mousemove", onMove);
  globalThis.addEventListener("mouseup", stopWatching, { once: true });
}
