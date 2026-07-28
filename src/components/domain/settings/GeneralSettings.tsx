import {
  useWorkspaceStore,
  type BranchSwitchMode,
  type CommitButtonMode,
  type TabLayout,
} from '@/stores/workspaceStore'
import { Button } from '@/components/ui/button'
import { commands } from '@/lib/bindings'
import { unwrap } from '@/lib/queryKeys'
import { useTutorialStore } from '@/stores/tutorialStore'
import { FolderSetting, SettingRow } from './SettingRow'
import { IdentitySetting } from './IdentitySetting'
import { EditorSetting } from './EditorSetting'
import { ResetToDefaults } from './ResetToDefaults'

const selectClass =
  'h-8 w-full rounded-md border border-input bg-background px-2 text-xs text-foreground outline-none focus:border-ring'

const branchSwitchHints: Record<BranchSwitchMode, string> = {
  auto_stash: 'Your changes are stashed, then reapplied on the new branch. If they conflict, the stash is kept as a backup.',
  carry: 'Your changes move to the new branch. The switch is refused if a change would be overwritten.',
  refuse: 'Switching is blocked while you have uncommitted changes.',
}

const commitButtonHints: Record<CommitButtonMode, string> = {
  commit: 'The commit button just commits. You can push later from the toolbar.',
  commit_push: 'The commit button commits, then pushes to your branch in one step.',
}

export function GeneralSettings() {
  const codeFolder = useWorkspaceStore((s) => s.codeFolder)
  const setCodeFolder = useWorkspaceStore((s) => s.setCodeFolder)
  const cloneDirectory = useWorkspaceStore((s) => s.cloneDirectory)
  const setCloneDirectory = useWorkspaceStore((s) => s.setCloneDirectory)
  const branchSwitchMode = useWorkspaceStore((s) => s.branchSwitchMode)
  const setBranchSwitchMode = useWorkspaceStore((s) => s.setBranchSwitchMode)
  const commitButtonMode = useWorkspaceStore((s) => s.commitButtonMode)
  const setCommitButtonMode = useWorkspaceStore((s) => s.setCommitButtonMode)
  const enableWorktrees = useWorkspaceStore((s) => s.enableWorktrees)
  const setEnableWorktrees = useWorkspaceStore((s) => s.setEnableWorktrees)
  const tabLayout = useWorkspaceStore((s) => s.tabLayout)
  const setTabLayout = useWorkspaceStore((s) => s.setTabLayout)
  const horizontalTabRow = useWorkspaceStore((s) => s.horizontalTabRow)
  const setHorizontalTabRow = useWorkspaceStore((s) => s.setHorizontalTabRow)

  return (
    <div>
      <SettingRow
        label="Your name and email"
        searchId="git-identity"
        hint="Goes on every commit you make, so other people can see who did the work. Shared with git itself, so other git tools see it too."
      >
        <IdentitySetting />
      </SettingRow>
      <SettingRow label="Code folder" searchId="code-folder" hint="Scanned for repositories to quick-launch from the open dialog.">
        <FolderSetting value={codeFolder} placeholder="e.g. C:\code" onCommit={setCodeFolder} />
      </SettingRow>
      <SettingRow
        label="Default clone directory"
        searchId="clone-directory"
        hint="Where new clones go. Falls back to the code folder when empty."
      >
        <FolderSetting
          value={cloneDirectory}
          placeholder={codeFolder ?? 'Not set'}
          onCommit={setCloneDirectory}
        />
      </SettingRow>
      <SettingRow
        label="When switching branches"
        searchId="branch-switch-mode"
        hint={branchSwitchHints[branchSwitchMode]}
      >
        <select
          className={selectClass}
          value={branchSwitchMode}
          onChange={(e) => setBranchSwitchMode(e.target.value as BranchSwitchMode)}
        >
          <option value="auto_stash">Stash my changes and bring them along</option>
          <option value="carry">Carry my changes over (like git checkout)</option>
          <option value="refuse">Don't let me switch with changes</option>
        </select>
      </SettingRow>
      <SettingRow label="Commit button" searchId="commit-button-mode" hint={commitButtonHints[commitButtonMode]}>
        <select
          className={selectClass}
          value={commitButtonMode}
          onChange={(e) => setCommitButtonMode(e.target.value as CommitButtonMode)}
        >
          <option value="commit">Commit only</option>
          <option value="commit_push">Commit and push</option>
        </select>
      </SettingRow>
      <SettingRow
        label="Open in editor"
        searchId="editor"
        hint="Which editor the open button and the file right-click menu use. Only editors found on this computer are listed."
      >
        <EditorSetting />
      </SettingRow>
      <SettingRow
        label="Repository tabs"
        searchId="tab-layout"
        hint="Put repository tabs across the top or in a scrollable list on the left. Groups work in both layouts."
      >
        <select
          className={selectClass}
          value={tabLayout}
          onChange={(event) => setTabLayout(event.target.value as TabLayout)}
        >
          <option value="horizontal">Across the top</option>
          <option value="vertical">Down the left side</option>
        </select>
      </SettingRow>
      {tabLayout === 'horizontal' && (
        <SettingRow
          label="Tab row"
          searchId="tab-row"
          hint="Give the tabs a row of their own under the app bar, so long repository names have the full width to themselves."
        >
          <label className="flex cursor-pointer items-center gap-2 text-xs text-foreground">
            <input
              type="checkbox"
              checked={horizontalTabRow}
              onChange={(e) => setHorizontalTabRow(e.target.checked)}
              className="size-3.5 accent-[var(--gw-accent)]"
            />
            Put tabs on their own row
          </label>
        </SettingRow>
      )}
      <SettingRow
        label="Worktrees"
        searchId="worktrees"
        hint="Worktrees let you check out more than one branch at once, each in its own folder. An advanced feature, off by default. Turns on by itself if this repo already uses them."
      >
        <label className="flex cursor-pointer items-center gap-2 text-xs text-foreground">
          <input
            type="checkbox"
            checked={enableWorktrees}
            onChange={(e) => setEnableWorktrees(e.target.checked)}
            className="size-3.5 accent-[var(--gw-accent)]"
          />
          Enable worktrees
        </label>
      </SettingRow>
      <SettingRow
        label="Hands-on tour"
        searchId="tutorial"
        hint="Builds a small practice repository and walks you through the gestures that are quick but easy to miss: double-click, drag, and right-click. Deleted when you finish."
      >
        <ReplayTutorialButton />
      </SettingRow>
      <ResetToDefaults group="general" />
    </div>
  )
}

/**
 * Re-runs the practice tour from Settings.
 *
 * Worth having because the tour is offered exactly once, at the end of
 * onboarding, which is the moment a new user is least able to judge whether
 * they want it. This is the second chance.
 */
function ReplayTutorialButton() {
  const starting = useTutorialStore((s) => s.starting)
  const active = useTutorialStore((s) => s.active)

  const run = () => {
    void useTutorialStore.getState().start(async (path) => {
      const repo = unwrap(await commands.openRepo(path))
      useWorkspaceStore.getState().addRepo(repo)
      return repo.id
    })
  }

  return (
    <Button size="sm" variant="secondary" onClick={run} disabled={starting || active}>
      {starting ? 'Setting up...' : 'Start the tour'}
    </Button>
  )
}
