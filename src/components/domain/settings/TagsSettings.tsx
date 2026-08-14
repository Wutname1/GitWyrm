import { useWorkspaceStore, type TagPushDefault } from '@/stores/workspaceStore'
import { SettingRow, SettingsGroup } from './SettingRow'
import { ResetToDefaults } from './ResetToDefaults'

const selectClass =
  'h-8 w-full rounded-md border border-input bg-background px-2 text-xs text-foreground outline-none focus:border-ring'

const tagPushHints: Record<TagPushDefault, string> = {
  ask: 'After a push, you get asked whether to send any tags that are still only on your computer.',
  always: 'Tags are sent automatically whenever you push. You will not be asked.',
  never: 'Tags are never sent automatically. You can still send them one at a time from the tag list.',
}

export function TagsSettings() {
  const tagPushDefault = useWorkspaceStore((s) => s.tagPushDefault)
  const setTagPushDefault = useWorkspaceStore((s) => s.setTagPushDefault)
  const tagPushOnCreate = useWorkspaceStore((s) => s.tagPushOnCreate)
  const setTagPushOnCreate = useWorkspaceStore((s) => s.setTagPushOnCreate)
  const tagDeleteOnRemote = useWorkspaceStore((s) => s.tagDeleteOnRemote)
  const setTagDeleteOnRemote = useWorkspaceStore((s) => s.setTagDeleteOnRemote)

  return (
    <div>
      <SettingsGroup title="Sending tags" blurb="These defaults apply unless a repository has its own tag rules.">
        <SettingRow label="After pushing" searchId="tag-push-default" hint={tagPushHints[tagPushDefault]}>
          <select
            className={selectClass}
            value={tagPushDefault}
            onChange={(e) => setTagPushDefault(e.target.value as TagPushDefault)}
          >
            <option value="ask">Ask me about tags I have not sent</option>
            <option value="always">Always send my tags</option>
            <option value="never">Never send my tags</option>
          </select>
        </SettingRow>
      </SettingsGroup>

      <SettingsGroup title="New and deleted tags">
        <SettingRow
          label="New tags"
          searchId="tag-push-on-create"
          hint="Choose whether new tags start ready to send."
        >
          <label className="flex cursor-pointer items-center gap-2 text-xs text-foreground">
            <input
              type="checkbox"
              checked={tagPushOnCreate}
              onChange={(e) => setTagPushOnCreate(e.target.checked)}
              className="size-3.5 accent-[var(--gw-accent)]"
            />
            Send a new tag as soon as I make it
          </label>
        </SettingRow>

        <SettingRow
          label="Deleting tags"
          searchId="tag-delete-on-remote"
          hint="Choose whether deleting a tag also removes its remote copy."
        >
          <label className="flex cursor-pointer items-center gap-2 text-xs text-foreground">
            <input
              type="checkbox"
              checked={tagDeleteOnRemote}
              onChange={(e) => setTagDeleteOnRemote(e.target.checked)}
              className="size-3.5 accent-[var(--gw-accent)]"
            />
            Also remove the tag from the remote
          </label>
        </SettingRow>
      </SettingsGroup>
      <ResetToDefaults group="tags" />
    </div>
  )
}
