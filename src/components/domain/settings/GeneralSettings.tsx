import {
  usePrimaryCodeFolder,
  useWorkspaceStore,
  type BranchSwitchMode,
  type CommitButtonMode,
  type TabLayout,
} from '@/stores/workspaceStore'
import { Button } from '@/components/ui/button'
import { commands } from '@/lib/bindings'
import { unwrap } from '@/lib/queryKeys'
import { useTutorialStore } from '@/stores/tutorialStore'
import { TUTORIAL_ENABLED } from '@/lib/tutorialEnabled'
import { FolderSetting, SettingRow, SettingsGroup } from './SettingRow'
import { CodeFoldersSetting } from './CodeFoldersSetting'
import { IdentitySetting } from './IdentitySetting'
import { EditorSetting } from './EditorSetting'
import { ResetToDefaults } from './ResetToDefaults'

const selectClass =
  'h-8 w-full rounded-md border border-input bg-background px-2 text-xs text-foreground outline-none focus:border-ring'

const branchSwitchHints: Record<BranchSwitchMode, string> = {
  auto_stash: 'Save unfinished work, switch branches, then bring the work back.',
  carry: 'Move unfinished work with you. The switch stops if a file would be overwritten.',
  refuse: 'Stop the switch until you commit or discard your changes.',
}

const commitButtonHints: Record<CommitButtonMode, string> = {
  commit: 'Save the commit locally. Push later from the toolbar.',
  commit_push: 'Make the commit and send it to the remote in one step.',
}

export function GeneralSettings() {
  const primaryCodeFolder = usePrimaryCodeFolder()
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
      <SettingsGroup title="Commit identity" blurb="The details attached to new commits.">
        <SettingRow
          label="Your name and email"
          searchId="git-identity"
          hint="Shown on every commit you make."
        >
          <IdentitySetting />
        </SettingRow>
      </SettingsGroup>

      <SettingsGroup title="Project folders" blurb="Where GitWyrm finds and creates repositories.">
        <SettingRow
          label="Code folders"
          searchId="code-folder"
          hint="Places GitWyrm checks for repositories. The starred folder is your main one."
        >
          <CodeFoldersSetting />
        </SettingRow>
        <SettingRow
          label="New copies go here"
          searchId="clone-directory"
          hint="Default folder for cloned repositories. Uses your main code folder when empty."
        >
          <FolderSetting
            value={cloneDirectory}
            placeholder={primaryCodeFolder ?? 'Not set'}
            onCommit={setCloneDirectory}
          />
        </SettingRow>
      </SettingsGroup>

      <SettingsGroup title="Git actions" blurb="Choose what common buttons do.">
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
            <option value="auto_stash">Save my changes and bring them along</option>
            <option value="carry">Carry my changes over</option>
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
          hint="Used by Open and the file right-click menu."
        >
          <EditorSetting />
        </SettingRow>
      </SettingsGroup>

      <SettingsGroup title="Workspace" blurb="How repositories are arranged while you work.">
        <SettingRow
          label="Repository tabs"
          searchId="tab-layout"
          hint="Place open repositories across the top or down the left."
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
            hint="Give top tabs their own row so long names have more room."
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
          hint="Work on two branches at once in separate folders."
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
        {TUTORIAL_ENABLED && (
          <SettingRow
            label="Hands-on tour"
            searchId="tutorial"
            hint="Practice double-click, drag, and right-click in a temporary repository."
          >
            <ReplayTutorialButton />
          </SettingRow>
        )}
      </SettingsGroup>
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
