import { useWorkspaceStore, type TagPushDefault } from '@/stores/workspaceStore'
import { SettingRow } from './SettingRow'

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

  return (
    <div>
      <p className="pt-3 text-2xs leading-relaxed text-muted-foreground">
        A tag is a name you pin to one commit, usually to mark a release. Tags are not sent when you
        push, so they stay on your computer until you send them on purpose.
      </p>

      <SettingRow label="After pushing" hint={tagPushHints[tagPushDefault]}>
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

      <SettingRow
        label="New tags"
        hint="Sets how the box starts out in the New tag window. You can still change it for any one tag."
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
    </div>
  )
}
