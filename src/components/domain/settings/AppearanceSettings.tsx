import { SettingRow } from './SettingRow'

export function AppearanceSettings() {
  return (
    <div>
      <SettingRow label="Theme" hint="Accent and density options land in a later release.">
        <div className="text-xs text-sub">GitWyrm Dark</div>
      </SettingRow>
    </div>
  )
}
