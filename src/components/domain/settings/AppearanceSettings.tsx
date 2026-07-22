import { Minus, Plus } from 'lucide-react'
import { SettingRow } from './SettingRow'
import { Button } from '@/components/ui/button'
import { Slider } from '@/components/ui/slider'
import {
  DEFAULT_UI_SCALE,
  MAX_UI_SCALE,
  MIN_UI_SCALE,
  UI_SCALE_STEP,
  useWorkspaceStore,
} from '@/stores/workspaceStore'
import { ChangeSizeSettings } from './ChangeSizeSettings'

function ZoomSetting() {
  const uiScale = useWorkspaceStore((s) => s.uiScale)
  const setUiScale = useWorkspaceStore((s) => s.setUiScale)
  const percent = Math.round(uiScale * 100)

  return (
    <div className="flex max-w-sm flex-col gap-2">
      <div className="flex items-center gap-2">
        <Button
          variant="outline"
          size="icon-sm"
          onClick={() => setUiScale(uiScale - UI_SCALE_STEP)}
          disabled={uiScale <= MIN_UI_SCALE}
          aria-label="Make everything smaller"
        >
          <Minus />
        </Button>
        <Slider
          value={[percent]}
          min={MIN_UI_SCALE * 100}
          max={MAX_UI_SCALE * 100}
          step={UI_SCALE_STEP * 100}
          onValueChange={([v]) => setUiScale(v / 100)}
          className="flex-1"
          aria-label="App zoom"
        />
        <Button
          variant="outline"
          size="icon-sm"
          onClick={() => setUiScale(uiScale + UI_SCALE_STEP)}
          disabled={uiScale >= MAX_UI_SCALE}
          aria-label="Make everything bigger"
        >
          <Plus />
        </Button>
        <span className="w-10 flex-none text-right font-mono text-xs text-sub">{percent}%</span>
      </div>
      <Button
        variant="ghost"
        size="sm"
        className="self-start"
        onClick={() => setUiScale(DEFAULT_UI_SCALE)}
        disabled={uiScale === DEFAULT_UI_SCALE}
      >
        Reset to 100%
      </Button>
    </div>
  )
}

export function AppearanceSettings() {
  const showRepoIcons = useWorkspaceStore((s) => s.showRepoIcons)
  const setShowRepoIcons = useWorkspaceStore((s) => s.setShowRepoIcons)
  const tabIconOnly = useWorkspaceStore((s) => s.tabIconOnly)
  const setTabIconOnly = useWorkspaceStore((s) => s.setTabIconOnly)

  return (
    <div>
      <SettingRow label="Theme" hint="Accent and density options land in a later release.">
        <div className="text-xs text-sub">GitWyrm Dark</div>
      </SettingRow>
      <SettingRow
        label="Repository icons"
        hint="Shows a favicon or logo beside each repository tab when one is available."
      >
        <label className="flex cursor-pointer items-center gap-2 text-xs text-foreground">
          <input
            type="checkbox"
            checked={showRepoIcons}
            onChange={(event) => setShowRepoIcons(event.target.checked)}
            className="size-3.5 accent-[var(--gw-accent)]"
          />
          Show icons in repository tabs
        </label>
      </SettingRow>
      <SettingRow
        label="Icon-only tabs"
        hint="Fits more repositories by hiding tab names. Point at a tab to expand its name."
      >
        <label className="flex cursor-pointer items-center gap-2 text-xs text-foreground">
          <input
            type="checkbox"
            checked={tabIconOnly}
            onChange={(event) => setTabIconOnly(event.target.checked)}
            className="size-3.5 accent-[var(--gw-accent)]"
          />
          Show only icons in repository tabs
        </label>
      </SettingRow>
      <SettingRow
        label="Commit change size"
        hint="See how large each commit is without opening it."
      >
        <ChangeSizeSettings />
      </SettingRow>
      <SettingRow label="App zoom" hint="Makes everything in GitWyrm bigger or smaller.">
        <ZoomSetting />
      </SettingRow>
    </div>
  )
}
