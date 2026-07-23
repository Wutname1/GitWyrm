import { SectionItemRow } from 'gitwyrm-mockup'
import { ArrowLeftRight } from 'lucide-react'
import type { SidebarSectionData, SectionItem } from '@/lib/types'

const localSection: SidebarSectionData = {
  key: 'local',
  label: 'Branches',
  type: 'branch',
  items: [],
}

const prSection: SidebarSectionData = {
  key: 'prs',
  label: 'Pull Requests',
  type: 'pr',
  items: [],
}

const stashSection: SidebarSectionData = {
  key: 'stashes',
  label: 'Stashes',
  type: 'stash',
  items: [],
}

function Panel({ children }: { children: React.ReactNode }) {
  return (
    <div style={{ padding: 20 }}>
      <div style={{ width: 280, border: '1px solid var(--gw-border)', borderRadius: 6 }}>
        {children}
      </div>
    </div>
  )
}

export function CurrentBranch() {
  const item: SectionItem = { name: 'feature/commit-graph', meta: '↑2 ↓1' }
  return (
    <Panel>
      <SectionItemRow section={localSection} item={item} isCurrent onClick={() => {}} />
    </Panel>
  )
}

export function BranchList() {
  const items: SectionItem[] = [
    { name: 'main', meta: '↑1' },
    { name: 'feature/commit-graph' },
    { name: 'fix/secret-values', meta: 'NEW' },
  ]
  return (
    <Panel>
      {items.map((item, i) => (
        <SectionItemRow
          key={item.name}
          section={localSection}
          item={item}
          isCurrent={i === 1}
          onClick={() => {}}
        />
      ))}
    </Panel>
  )
}

export function PullRequest() {
  const item: SectionItem = { name: 'Add commit graph rendering', meta: '#142', id: 142, state: 'open' }
  const merged: SectionItem = { name: 'Fix conflict resolver', meta: '#138', id: 138, state: 'merged' }
  return (
    <Panel>
      <SectionItemRow section={prSection} item={item} isCurrent={false} onClick={() => {}} />
      <SectionItemRow section={prSection} item={merged} isCurrent={false} onClick={() => {}} />
    </Panel>
  )
}

export function StashRow() {
  const item: SectionItem = { name: 'WIP on main: graph tweaks', meta: '2h ago', sha: 'a1b2c3d' }
  return (
    <Panel>
      <SectionItemRow section={stashSection} item={item} isCurrent={false} onClick={() => {}} />
    </Panel>
  )
}

export function WithHoverAction() {
  const item: SectionItem = { name: 'release/2.1', meta: '↓4' }
  return (
    <Panel>
      <SectionItemRow
        section={localSection}
        item={item}
        isCurrent={false}
        onClick={() => {}}
        hoverAction={{ icon: <ArrowLeftRight size={12} />, title: 'Switch to branch', onClick: () => {} }}
      />
    </Panel>
  )
}

export function Pending() {
  const item: SectionItem = { name: 'feature/rebase-ui' }
  return (
    <Panel>
      <SectionItemRow
        section={localSection}
        item={item}
        isCurrent={false}
        onClick={() => {}}
        pending
        pendingLabel="Checking out…"
      />
    </Panel>
  )
}
