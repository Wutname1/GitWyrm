import { FolderGit2, RotateCcw, Tags } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { useUiStore } from '@/stores/uiStore'
import {
  useActiveRepo,
  useWorkspaceStore,
  type TagPushDefault,
} from '@/stores/workspaceStore'
import { SettingRow } from './SettingRow'
import { ResetToDefaults } from './ResetToDefaults'

const selectClass =
  'h-8 w-full rounded-md border border-input bg-background px-2 text-xs text-foreground outline-none focus:border-ring disabled:cursor-not-allowed disabled:opacity-60'

const tagPushLabels: Record<TagPushDefault, string> = {
  ask: 'Ask me about tags I have not sent',
  always: 'Always send my tags',
  never: 'Never send my tags',
}

export function RepositoryTagsSettings() {
  const repo = useActiveRepo()
  const appPushDefault = useWorkspaceStore((s) => s.tagPushDefault)
  const appPushOnCreate = useWorkspaceStore((s) => s.tagPushOnCreate)
  const override = useWorkspaceStore((s) => (repo ? s.tagOverridesByRepo[repo.path] : undefined))
  const setRepoTagOverride = useWorkspaceStore((s) => s.setRepoTagOverride)
  const clearRepoTagOverride = useWorkspaceStore((s) => s.clearRepoTagOverride)

  if (!repo) {
    return (
      <div className="mt-4 grid place-items-center rounded-xl border border-dashed border-border bg-panel/40 px-6 py-12 text-center">
        <div className="grid size-12 place-items-center rounded-xl border border-border bg-panel2 text-muted-foreground">
          <FolderGit2 size={22} strokeWidth={1.6} />
        </div>
        <h3 className="mt-3 text-sm font-semibold text-foreground">No repository selected</h3>
        <p className="mt-1 max-w-sm text-2xs leading-relaxed text-muted-foreground">
          Select a repository tab first. Its own tag rules will appear here.
        </p>
      </div>
    )
  }

  const hasOverride = override != null
  // The values shown in the controls: the repo's own choice, or the app default it inherits.
  const effectivePushDefault = override?.pushDefault ?? appPushDefault
  const effectivePushOnCreate = override?.pushOnCreate ?? appPushOnCreate

  const enableOverride = (on: boolean) => {
    if (on) {
      // Seed the override from what this repo is already doing, so turning it on
      // changes nothing until the user picks something different.
      setRepoTagOverride(repo.path, {
        pushDefault: appPushDefault,
        pushOnCreate: appPushOnCreate,
      })
    } else {
      clearRepoTagOverride(repo.path)
    }
  }

  return (
    <div>
      <p className="pt-3 text-2xs leading-relaxed text-muted-foreground">
        By default this repository uses your application tag settings. Turn on a custom setting
        below to give <span className="font-medium text-foreground">{repo.name}</span> its own tag
        rules. Other repositories are not affected.
      </p>

      <SettingRow
        label="Custom tag settings"
        hint="When off, this repository follows whatever you set under Application > Tags."
      >
        <label className="flex cursor-pointer items-center gap-2 text-xs text-foreground">
          <input
            type="checkbox"
            checked={hasOverride}
            onChange={(e) => enableOverride(e.target.checked)}
            className="size-3.5 accent-[var(--gw-accent)]"
          />
          Use custom tag settings for this repository
        </label>
      </SettingRow>

      {!hasOverride ? (
        <div className="mt-1 flex items-start gap-3 rounded-lg border border-border bg-panel2 px-3.5 py-3">
          <Tags size={15} className="mt-0.5 flex-none text-muted-foreground" />
          <div className="min-w-0 text-2xs leading-relaxed text-muted-foreground">
            <div className="font-medium text-foreground">Following your application settings</div>
            <div className="mt-1">
              After pushing: {tagPushLabels[appPushDefault].toLowerCase()}.
            </div>
            <div>
              New tags start {appPushOnCreate ? 'with the send box checked' : 'with the send box unchecked'}.
            </div>
            <Button
              variant="link"
              size="xs"
              className="mt-1.5 h-auto px-0"
              onClick={() => useUiStore.getState().showSettings('tags')}
            >
              Open application tag settings
            </Button>
          </div>
        </div>
      ) : (
        <>
          <SettingRow
            label="After pushing"
            hint="What this repository does with local-only tags after a push."
          >
            <select
              className={selectClass}
              value={effectivePushDefault}
              onChange={(e) =>
                setRepoTagOverride(repo.path, { pushDefault: e.target.value as TagPushDefault })
              }
            >
              <option value="ask">Ask me about tags I have not sent</option>
              <option value="always">Always send my tags</option>
              <option value="never">Never send my tags</option>
            </select>
          </SettingRow>

          <SettingRow
            label="New tags"
            hint="How the send box starts out in the New tag window for this repository."
          >
            <label className="flex cursor-pointer items-center gap-2 text-xs text-foreground">
              <input
                type="checkbox"
                checked={effectivePushOnCreate}
                onChange={(e) =>
                  setRepoTagOverride(repo.path, { pushOnCreate: e.target.checked })
                }
                className="size-3.5 accent-[var(--gw-accent)]"
              />
              Send a new tag as soon as I make it
            </label>
          </SettingRow>

          <Button
            variant="ghost"
            size="xs"
            className="mt-1"
            onClick={() => clearRepoTagOverride(repo.path)}
          >
            <RotateCcw size={12} />
            Follow application settings instead
          </Button>
        </>
      )}

      <ResetToDefaults
        label="Reset this repository's tag rules"
        message={`${repo.name} now follows your application tag settings`}
        disabled={!hasOverride}
        onReset={() => {
          const prior = override
          clearRepoTagOverride(repo.path)
          return prior ? () => setRepoTagOverride(repo.path, prior) : null
        }}
      />
    </div>
  )
}
