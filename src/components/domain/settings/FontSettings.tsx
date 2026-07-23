import { useState } from 'react'
import { Minus, Plus } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Slider } from '@/components/ui/slider'
import {
  DEFAULT_FONT_SIZE,
  DEFAULT_FONT_WEIGHT,
  FONT_SIZE_STEP,
  FONT_WEIGHT_STEP,
  MAX_FONT_SIZE,
  MAX_FONT_WEIGHT,
  MIN_FONT_SIZE,
  MIN_FONT_WEIGHT,
  useWorkspaceStore,
} from '@/stores/workspaceStore'
import {
  BUNDLED_FONTS,
  DEFAULT_FONT_ID,
  canQuerySystemFonts,
  fontStack,
  loadSystemFonts,
  type FontOption,
} from '@/lib/fonts'

const selectClass =
  'h-8 w-full rounded-md border border-input bg-background px-2 text-xs text-foreground outline-none focus:border-ring'

const WEIGHT_LABELS: Record<number, string> = {
  300: 'Light',
  350: 'Light',
  400: 'Normal',
  450: 'Normal',
  500: 'Medium',
  550: 'Semibold',
  600: 'Semibold',
}

function FontFamilySetting() {
  const fontFamily = useWorkspaceStore((s) => s.fontFamily)
  const setFontFamily = useWorkspaceStore((s) => s.setFontFamily)
  const [systemFonts, setSystemFonts] = useState<FontOption[]>([])
  const [loadingSystem, setLoadingSystem] = useState(false)

  // The Local Font Access API needs a user gesture to prompt, so system fonts
  // load on demand from the button below rather than automatically.
  const requestSystemFonts = async () => {
    setLoadingSystem(true)
    setSystemFonts(await loadSystemFonts())
    setLoadingSystem(false)
  }

  // A previously-saved system font may not be in the bundled list; make sure the
  // dropdown can still show it as the selected value.
  const savedIsKnown =
    BUNDLED_FONTS.some((f) => f.id === fontFamily) ||
    systemFonts.some((f) => f.id === fontFamily)

  return (
    <div className="flex max-w-sm flex-col gap-2">
      <select
        className={selectClass}
        value={fontFamily}
        onChange={(e) => setFontFamily(e.target.value)}
        style={{ fontFamily: fontStack(fontFamily, systemFonts) }}
      >
        <optgroup label="Included with GitWyrm">
          {BUNDLED_FONTS.map((f) => (
            <option key={f.id} value={f.id} style={{ fontFamily: f.stack }}>
              {f.name}
            </option>
          ))}
        </optgroup>
        {!savedIsKnown && (
          <optgroup label="Current">
            <option value={fontFamily}>{fontFamily}</option>
          </optgroup>
        )}
        {systemFonts.length > 0 && (
          <optgroup label="Installed on this PC">
            {systemFonts.map((f) => (
              <option key={f.id} value={f.id} style={{ fontFamily: f.stack }}>
                {f.name}
              </option>
            ))}
          </optgroup>
        )}
      </select>
      {systemFonts.length === 0 && canQuerySystemFonts() && (
        <Button
          variant="ghost"
          size="sm"
          className="self-start"
          onClick={requestSystemFonts}
          disabled={loadingSystem}
        >
          {loadingSystem ? 'Loading fonts…' : 'Choose an installed font…'}
        </Button>
      )}
    </div>
  )
}

function FontSizeSetting() {
  const fontSize = useWorkspaceStore((s) => s.fontSize)
  const setFontSize = useWorkspaceStore((s) => s.setFontSize)
  // Show a familiar pixel size (rem * 16), not the raw rem value.
  const px = Math.round(fontSize * 16 * 10) / 10

  return (
    <div className="flex max-w-sm flex-col gap-2">
      <div className="flex items-center gap-2">
        <Button
          variant="outline"
          size="icon-sm"
          onClick={() => setFontSize(fontSize - FONT_SIZE_STEP)}
          disabled={fontSize <= MIN_FONT_SIZE}
          aria-label="Smaller text"
        >
          <Minus />
        </Button>
        <Slider
          value={[fontSize]}
          min={MIN_FONT_SIZE}
          max={MAX_FONT_SIZE}
          step={FONT_SIZE_STEP}
          onValueChange={([v]) => setFontSize(v)}
          className="flex-1"
          aria-label="Text size"
        />
        <Button
          variant="outline"
          size="icon-sm"
          onClick={() => setFontSize(fontSize + FONT_SIZE_STEP)}
          disabled={fontSize >= MAX_FONT_SIZE}
          aria-label="Bigger text"
        >
          <Plus />
        </Button>
        <span className="w-12 flex-none text-right font-mono text-xs text-sub">{px}px</span>
      </div>
      <Button
        variant="ghost"
        size="sm"
        className="self-start"
        onClick={() => setFontSize(DEFAULT_FONT_SIZE)}
        disabled={fontSize === DEFAULT_FONT_SIZE}
      >
        Reset to default size
      </Button>
    </div>
  )
}

function FontWeightSetting() {
  const fontWeight = useWorkspaceStore((s) => s.fontWeight)
  const setFontWeight = useWorkspaceStore((s) => s.setFontWeight)
  const label = WEIGHT_LABELS[fontWeight] ?? String(fontWeight)

  return (
    <div className="flex max-w-sm flex-col gap-2">
      <div className="flex items-center gap-2">
        <Slider
          value={[fontWeight]}
          min={MIN_FONT_WEIGHT}
          max={MAX_FONT_WEIGHT}
          step={FONT_WEIGHT_STEP}
          onValueChange={([v]) => setFontWeight(v)}
          className="flex-1"
          aria-label="Text weight"
        />
        <span
          className="w-20 flex-none text-right text-xs text-sub"
          style={{ fontWeight }}
        >
          {label}
        </span>
      </div>
      <Button
        variant="ghost"
        size="sm"
        className="self-start"
        onClick={() => setFontWeight(DEFAULT_FONT_WEIGHT)}
        disabled={fontWeight === DEFAULT_FONT_WEIGHT}
      >
        Reset to default weight
      </Button>
    </div>
  )
}

/** Restores font family, size, and weight to their defaults in one click. */
function useResetFonts() {
  const setFontFamily = useWorkspaceStore((s) => s.setFontFamily)
  const setFontSize = useWorkspaceStore((s) => s.setFontSize)
  const setFontWeight = useWorkspaceStore((s) => s.setFontWeight)
  return () => {
    setFontFamily(DEFAULT_FONT_ID)
    setFontSize(DEFAULT_FONT_SIZE)
    setFontWeight(DEFAULT_FONT_WEIGHT)
  }
}

export { FontFamilySetting, FontSizeSetting, FontWeightSetting, useResetFonts }
