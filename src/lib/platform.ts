/**
 * Which operating system the app is running on.
 *
 * Read from `navigator.platform` rather than `@tauri-apps/plugin-os`: this is
 * synchronous, needs no capability grant and no IPC round-trip, and the only
 * thing it is used for is hiding UI that the backend cannot honour. The
 * feedback reporter already reads the same field.
 *
 * `navigator.platform` is deprecated in the wider web platform but is present
 * and stable in every webview GitWyrm ships against (WebView2 on Windows,
 * WebKitGTK on Linux). It is checked with a prefix rather than an exact match
 * because the value differs across those runtimes - "Win32" even on 64-bit
 * Windows, "Linux x86_64" on GTK.
 */

function raw(): string {
  // Guard the whole read: a webview without a navigator (or a test environment
  // that stubs a partial one) must not take the app down over a UI detail.
  try {
    return navigator?.platform ?? ''
  } catch {
    return ''
  }
}

/** True on Windows. Windows-only features gate on this. */
export function isWindows(): boolean {
  return raw().toLowerCase().startsWith('win')
}

/** True on Linux. */
export function isLinux(): boolean {
  return raw().toLowerCase().startsWith('linux')
}

/**
 * The name of the git executable as a user would type it, for labels like
 * "Browse for git.exe". Saying ".exe" on Linux would send someone looking for
 * a file that does not exist there.
 */
export function executableName(tool: string): string {
  return isWindows() ? `${tool}.exe` : tool
}
