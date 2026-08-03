import { normalizePath, pathKey, samePath } from '@/lib/paths'

/** A folder GitWyrm scans for repositories. */
export interface CodeFolder {
  path: string
  /** Short name shown beside the path, or null to show the path alone. */
  label: string | null
  /**
   * Default destination for clones and new repositories, and the folder sorted
   * first. Exactly one folder in a non-empty list carries this.
   */
  primary: boolean
}

/** The shape `code_folders` takes in settings.json. Every field is optional. */
export interface StoredCodeFolder {
  path?: string
  label?: string | null
  primary?: boolean
}

/**
 * Guarantee exactly one primary folder: the first flagged one wins, and a list
 * that lost its primary (folder removed, or settings.json hand-edited) promotes
 * its first entry.
 *
 * A non-empty list must never be without a primary -- clone and new-repo read it
 * for their default destination, so losing it silently sends new work elsewhere.
 */
export function withOnePrimary(folders: CodeFolder[]): CodeFolder[] {
  if (folders.length === 0) return folders
  const flagged = folders.findIndex((folder) => folder.primary)
  const winner = flagged === -1 ? 0 : flagged
  return folders.map((folder, index) => ({ ...folder, primary: index === winner }))
}

/**
 * Rebuild the watched-folder list from settings, dropping blanks and duplicate
 * paths and guaranteeing exactly one primary folder.
 *
 * `legacy` is the pre-list `code_folder` value. When the list is empty but that
 * folder is set, the user is upgrading: seed the list from it so their watched
 * folder survives untouched. Same single-value-to-collection upgrade as
 * `normalizeAiModels`.
 */
export function deserializeCodeFolders(
  stored: StoredCodeFolder[] | undefined,
  legacy: string | null | undefined,
): CodeFolder[] {
  const byKey = new Map<string, CodeFolder>()
  for (const folder of stored ?? []) {
    const path = folder.path?.trim()
    if (!path) continue
    const key = pathKey(path)
    if (byKey.has(key)) continue
    byKey.set(key, {
      path: normalizePath(path),
      label: folder.label?.trim() || null,
      primary: folder.primary === true,
    })
  }

  if (byKey.size === 0 && legacy?.trim()) {
    byKey.set(pathKey(legacy), {
      path: normalizePath(legacy),
      label: null,
      primary: true,
    })
  }

  return withOnePrimary([...byKey.values()])
}

/** The starred folder's path, or null when no folder is watched. */
export function primaryPathOf(folders: CodeFolder[]): string | null {
  return folders.find((folder) => folder.primary)?.path ?? null
}

/** Sort the primary folder first, leaving the rest in their existing order. */
export function primaryFirst(folders: CodeFolder[]): CodeFolder[] {
  return [...folders].sort((a, b) => Number(b.primary) - Number(a.primary))
}

/** Add `path` to the list, or return it unchanged when already watched. */
export function addFolder(folders: CodeFolder[], path: string): CodeFolder[] {
  const trimmed = path.trim()
  if (!trimmed) return folders
  const normalized = normalizePath(trimmed)
  if (folders.some((folder) => samePath(folder.path, normalized))) return folders
  return [
    ...folders,
    // The first folder watched must be primary: the list is never non-empty
    // without one.
    { path: normalized, label: null, primary: folders.length === 0 },
  ]
}

/** Drop `path`, promoting a new primary when the starred folder was the one removed. */
export function removeFolder(folders: CodeFolder[], path: string): CodeFolder[] {
  const remaining = folders.filter((folder) => !samePath(folder.path, path))
  if (remaining.length === folders.length) return folders
  return withOnePrimary(remaining)
}

/** Star `path`, un-starring whichever folder held it. Unknown paths change nothing. */
export function setPrimaryFolder(folders: CodeFolder[], path: string): CodeFolder[] {
  if (!folders.some((folder) => samePath(folder.path, path))) return folders
  return folders.map((folder) => ({
    ...folder,
    primary: samePath(folder.path, path),
  }))
}

/** Rename a folder for display. A blank label clears back to showing the path. */
export function setFolderLabel(
  folders: CodeFolder[],
  path: string,
  label: string,
): CodeFolder[] {
  const trimmed = label.trim()
  return folders.map((folder) =>
    samePath(folder.path, path) ? { ...folder, label: trimmed || null } : folder,
  )
}
