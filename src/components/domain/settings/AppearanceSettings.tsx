import { List, Minus, Monitor, Moon, Plus, RotateCcw, Sun, Trees } from 'lucide-react'
import { toast } from 'sonner'
import { SettingRow, SettingsGroup } from './SettingRow'
import { Button } from '@/components/ui/button'
import { Slider } from '@/components/ui/slider'
import {
  DEFAULT_UI_SCALE,
  MAX_UI_SCALE,
  MIN_UI_SCALE,
  UI_SCALE_STEP,
  useWorkspaceStore,
} from '@/stores/workspaceStore'
import {
  THEMES,
  resolveMode,
  type ThemeId,
  type ThemeMode,
} from '@/lib/themes'
import { cn } from '@/lib/utils'
import { ChangeSizeSettings } from './ChangeSizeSettings'
import { ResetToDefaults } from './ResetToDefaults'
import {
  FontFamilySetting,
  FontSizeSetting,
  FontWeightSetting,
  useResetFonts,
} from './FontSettings'

const MODE_OPTIONS: { value: ThemeMode; label: string; icon: typeof Sun }[] = [
  { value: 'system', label: 'System', icon: Monitor },
  { value: 'light', label: 'Light', icon: Sun },
  { value: 'dark', label: 'Dark', icon: Moon },
]

function ModeSetting() {
  const themeMode = useWorkspaceStore((s) => s.themeMode)
  const setThemeMode = useWorkspaceStore((s) => s.setThemeMode)

  return (
    <div className="inline-flex overflow-hidden rounded-md border border-border">
      {MODE_OPTIONS.map(({ value, label, icon: Icon }) => (
        <button
          key={value}
          onClick={() => setThemeMode(value)}
          className={cn(
            'flex items-center gap-1.5 px-3 py-1.5 text-xs transition-colors',
            themeMode === value
              ? 'bg-[var(--gw-accent)] text-[var(--gw-accent-fg)]'
              : 'bg-panel2 text-sub hover:bg-panel3 hover:text-foreground',
          )}
        >
          <Icon size={13} />
          {label}
        </button>
      ))}
    </div>
  )
}

function ThemeSetting() {
  const theme = useWorkspaceStore((s) => s.theme)
  const themeMode = useWorkspaceStore((s) => s.themeMode)
  const setTheme = useWorkspaceStore((s) => s.setTheme)

  // Preview each theme in the mode the app is actually showing, so the swatches
  // match what the user sees.
  const systemPrefersDark =
    typeof window !== 'undefined' &&
    window.matchMedia('(prefers-color-scheme: dark)').matches
  const activeMode = resolveMode(themeMode, systemPrefersDark)

  const options: { id: ThemeId; name: string; note: string }[] = [
    {
      id: 'auto',
      name: 'Auto',
      note: 'Match the light or dark setting (Slate or Paper).',
    },
    ...THEMES.map((t) => ({ id: t.id as ThemeId, name: t.name, note: t.note })),
  ]

  return (
    <div className="grid max-w-xl grid-cols-2 gap-2">
      {options.map((opt) => {
        const def = opt.id === 'auto' ? null : THEMES.find((t) => t.id === opt.id)
        const swatch = def ? def[activeMode].surface : null
        const dot = def ? def[activeMode].accent.accent : 'var(--gw-accent)'
        const active = theme === opt.id
        return (
          <button
            key={opt.id}
            onClick={() => setTheme(opt.id)}
            className={cn(
              'flex items-center gap-2.5 rounded-lg border p-2.5 text-left transition-colors',
              active
                ? 'border-[var(--gw-accent)] bg-[var(--gw-accent-soft)]'
                : 'border-border bg-panel hover:bg-panel2',
            )}
          >
            <div className="flex flex-none items-center">
              {swatch ? (
                <>
                  <span
                    className="size-5 rounded-l border border-border"
                    style={{ background: swatch.bg }}
                  />
                  <span
                    className="size-5 border-y border-border"
                    style={{ background: swatch.panel2 }}
                  />
                  <span
                    className="flex size-5 items-center justify-center rounded-r border border-border"
                    style={{ background: swatch.panel3 }}
                  >
                    <span
                      className="size-2 rounded-full"
                      style={{ background: dot }}
                    />
                  </span>
                </>
              ) : (
                // Auto: split light/dark tile
                <span className="flex size-5 overflow-hidden rounded border border-border">
                  <span className="w-1/2 bg-[#1a1a1a]" />
                  <span className="w-1/2 bg-white" />
                </span>
              )}
            </div>
            <div className="min-w-0">
              <div className="text-xs font-semibold text-foreground">{opt.name}</div>
              <div className="truncate text-2xs text-muted-foreground">{opt.note}</div>
            </div>
          </button>
        )
      })}
    </div>
  )
}

function MintSetting() {
  const mintAccent = useWorkspaceStore((s) => s.mintAccent)
  const setMintAccent = useWorkspaceStore((s) => s.setMintAccent)

  return (
    <label className="flex cursor-pointer items-center gap-2 text-xs text-foreground">
      <input
        type="checkbox"
        checked={mintAccent}
        onChange={(event) => setMintAccent(event.target.checked)}
        className="size-3.5 accent-[var(--gw-accent)]"
      />
      Use the GitWyrm mint accent on every theme
    </label>
  )
}

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
  const changesViewMode = useWorkspaceStore((s) => s.changesViewMode)
  const setChangesViewMode = useWorkspaceStore((s) => s.setChangesViewMode)
  const resetWorkspaceLayout = useWorkspaceStore((s) => s.resetWorkspaceLayout)
  const resetFonts = useResetFonts()

  return (
    <div>
      <SettingsGroup title="Color">
        <SettingRow label="Mode" searchId="theme-mode" hint="Choose light, dark, or follow Windows.">
          <ModeSetting />
        </SettingRow>
        <SettingRow label="Theme" searchId="theme" hint="Pick the colors used across GitWyrm.">
          <ThemeSetting />
        </SettingRow>
        <SettingRow
          label="Mint accent"
          searchId="mint-accent"
          hint="Use mint for selections and active controls."
        >
          <MintSetting />
        </SettingRow>
      </SettingsGroup>

      <SettingsGroup title="Text">
        <SettingRow
          label="Font"
          searchId="font"
          hint="Use an included font or one installed on this computer."
        >
          <FontFamilySetting />
        </SettingRow>
        <SettingRow label="Text size" searchId="text-size" hint="Change text without resizing the whole interface.">
          <FontSizeSetting />
        </SettingRow>
        <SettingRow label="Text weight" searchId="text-weight" hint="Make labels lighter or stronger.">
          <FontWeightSetting />
        </SettingRow>
        <SettingRow label="Reset fonts" searchId="reset-fonts" hint="Put the font, size, and weight back to GitWyrm's defaults.">
          <Button variant="outline" size="sm" onClick={resetFonts}>
            Reset all font settings
          </Button>
        </SettingRow>
      </SettingsGroup>

      <SettingsGroup title="Repository tabs">
        <SettingRow
          label="Repository icons"
          searchId="repo-icons"
          hint="Show a small icon beside each repository name."
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
          searchId="icon-only-tabs"
          hint="Hide names until you point at a tab."
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
      </SettingsGroup>

      <SettingsGroup title="Changed files">
        <SettingRow
          label="Changed files"
          searchId="changes-view"
          hint="Group files by folder or show one simple list."
        >
          <div className="inline-flex rounded-md border border-border bg-background p-0.5">
            {([
              ['tree', 'Group by folder', Trees],
              ['list', 'One list', List],
            ] as const).map(([value, label, Icon]) => (
              <Button
                key={value}
                type="button"
                variant="ghost"
                size="xs"
                aria-pressed={changesViewMode === value}
                onClick={() => setChangesViewMode(value)}
                className={cn(
                  'gap-1.5 text-sub',
                  changesViewMode === value && 'bg-panel3 text-foreground shadow-sm',
                )}
              >
                <Icon size={12} />
                {label}
              </Button>
            ))}
          </div>
        </SettingRow>
        <SettingRow
          label="Commit change size"
          searchId="change-size"
          hint="Show added and removed line counts beside commits."
        >
          <ChangeSizeSettings />
        </SettingRow>
      </SettingsGroup>

      <SettingsGroup title="Window size">
        <SettingRow label="App zoom" searchId="app-zoom" hint="Resize the whole interface. Ctrl + 0 returns to 100%.">
          <ZoomSetting />
        </SettingRow>
        <SettingRow
          label="Panel sizes"
          searchId="restore-panel-sizes"
          hint="Return every resizable panel to its starting size."
        >
          <Button
            variant="outline"
            size="sm"
            onClick={() => {
              resetWorkspaceLayout()
              toast.success('Panel sizes were restored')
            }}
          >
            <RotateCcw size={13} />
            Restore panel sizes
          </Button>
        </SettingRow>
      </SettingsGroup>
      <ResetToDefaults group="appearance" />
    </div>
  )
}
