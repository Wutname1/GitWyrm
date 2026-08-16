// UI-only types. Backend-owned types live in the generated bindings.ts.

export type SectionKey =
  | 'local'
  | 'remote'
  | 'worktrees'
  | 'stashes'
  | 'specs'
  | 'prs'
  | 'issues'
  | 'tags'
  | 'submodules'

export type SectionType = 'branch' | 'remote' | 'tree' | 'stash' | 'pr' | 'issue' | 'tag'

export interface SectionItem {
  name: string
  meta?: string
  /** Tooltip for `meta`, when the short form needs explaining. */
  metaTitle?: string
  state?: 'open' | 'draft' | 'merged'
  /** GitHub PR or issue number, for pr/issue rows. */
  id?: number
  /** Browser page for a hosted item such as a pull request or issue. */
  webUrl?: string
  /** Stash commit sha, for stash rows. Stable where messages can repeat. */
  sha?: string
  /**
   * Marks the row as tied to whatever is selected elsewhere -- today, the pull
   * request the selected commit belongs to. Drawn as a quieter emphasis than
   * `isCurrent`, which means "this is the thing you have open".
   */
  linked?: boolean
  /** Why this row is linked, in plain language. Tooltip for the marker. */
  linkedTitle?: string
}

export interface SidebarSectionData {
  key: SectionKey
  label: string
  type: SectionType
  items: SectionItem[]
}
