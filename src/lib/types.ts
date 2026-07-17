// UI-only types. Backend-owned types live in the generated bindings.ts.

export type SectionKey = 'local' | 'remote' | 'worktrees' | 'stashes' | 'prs' | 'issues' | 'tags'

export type SectionType = 'branch' | 'remote' | 'tree' | 'stash' | 'pr' | 'issue' | 'tag'

export interface SectionItem {
  name: string
  meta?: string
  state?: 'open' | 'draft' | 'merged'
}

export interface SidebarSectionData {
  key: SectionKey
  label: string
  type: SectionType
  items: SectionItem[]
}
