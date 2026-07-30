/**
 * Which UI this webview should render.
 *
 * Both windows load the same `index.html`; the Spec Desk's URL carries
 * `?window=spec-desk&repo=<id>`. Routing on that keeps one bundle, one dev
 * server, and one set of bindings -- a second entry point would be another thing
 * to keep in sync for no gain.
 */
export interface WindowMode {
  kind: 'main' | 'spec-desk'
  /** Repository the Desk is for. Null in the main window. */
  repoId: string | null
  /**
   * Working directory of that repository. The backend already knows it when it
   * opens the window, and passing it along saves the Desk -- whose store starts
   * empty -- from having to look it up before it can open anything.
   */
  repoPath: string | null
  /**
   * Change to select on arrival, when the Desk was opened from one. The
   * selection broadcast cannot reach a window that does not exist yet, so it
   * arrives in the URL instead. Null when the Desk was opened on its own.
   */
  changeId: string | null
}

export function readWindowMode(): WindowMode {
  const params = new URLSearchParams(window.location.search)
  if (params.get('window') === 'spec-desk') {
    return {
      kind: 'spec-desk',
      repoId: params.get('repo'),
      repoPath: params.get('path'),
      changeId: params.get('change'),
    }
  }
  return { kind: 'main', repoId: null, repoPath: null, changeId: null }
}

/** True in the Spec Desk window. Cheap enough to call during render. */
export function isSpecDeskWindow(): boolean {
  return readWindowMode().kind === 'spec-desk'
}
