import { Input } from '@/components/ui/input'
import { useWorkspaceStore } from '@/stores/workspaceStore'
import { FolderSetting, SettingRow } from './SettingRow'

export function GeneralSettings() {
  const codeFolder = useWorkspaceStore((s) => s.codeFolder)
  const setCodeFolder = useWorkspaceStore((s) => s.setCodeFolder)
  const cloneDirectory = useWorkspaceStore((s) => s.cloneDirectory)
  const setCloneDirectory = useWorkspaceStore((s) => s.setCloneDirectory)

  return (
    <div>
      <SettingRow label="Git executable" hint="Used for fetch, pull, push, and clone.">
        <Input defaultValue="git" className="h-8 bg-background font-mono text-xs" />
      </SettingRow>
      <SettingRow label="Code folder" hint="Scanned for repositories to quick-launch from the open dialog.">
        <FolderSetting value={codeFolder} placeholder="e.g. C:\code" onCommit={setCodeFolder} />
      </SettingRow>
      <SettingRow
        label="Default clone directory"
        hint="Where new clones go. Falls back to the code folder when empty."
      >
        <FolderSetting
          value={cloneDirectory}
          placeholder={codeFolder ?? 'Not set'}
          onCommit={setCloneDirectory}
        />
      </SettingRow>
    </div>
  )
}
