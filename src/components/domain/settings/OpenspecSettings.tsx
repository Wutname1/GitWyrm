import { useEffect, useRef, useState } from 'react'
import { Check, RotateCcw } from 'lucide-react'
import { ResetToDefaults } from './ResetToDefaults'
import { SettingRow, SettingsGroup, settingRowClass, useRevealHighlight } from './SettingRow'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { useOpenspecDefaultArchiveTemplate } from '@/hooks/useOpenspec'
import { useWorkspaceStore } from '@/stores/workspaceStore'

export function OpenspecSettings() {
  return (
    <div>
      <SettingsGroup title="Spec Desk">
        <SpecDeskSetting />
        <ArchiveCommitTemplateSetting />
      </SettingsGroup>
      <SettingsGroup title="Warnings">
        <ConfirmationSetting kind="archive" />
        <ConfirmationSetting kind="delete" />
      </SettingsGroup>
      <ResetToDefaults group="openspec" label="Reset OpenSpec settings to default" />
    </div>
  )
}

/** Restore either prompt after the user chose "Don't ask again" at the action. */
function ConfirmationSetting({ kind }: { kind: 'archive' | 'delete' }) {
  const skip = useWorkspaceStore((s) =>
    kind === 'archive' ? s.openspecArchiveWithoutAsking : s.openspecDeleteWithoutAsking
  )
  const setSkip = useWorkspaceStore((s) =>
    kind === 'archive' ? s.setOpenspecArchiveWithoutAsking : s.setOpenspecDeleteWithoutAsking
  )
  const label = kind === 'archive' ? 'Before archiving' : 'Before deleting'
  const verb = kind === 'archive' ? 'archiving a change' : 'deleting a change'

  return (
    <SettingRow
      label={label}
      searchId={`openspec-${kind}-confirmation`}
      hint={`The warning explains what will happen before ${verb}.`}
    >
      {skip ? (
        <Button variant="secondary" size="sm" onClick={() => setSkip(false)}>
          Ask me again
        </Button>
      ) : (
        <span className="flex items-center gap-1.5 text-xs text-accent-text">
          <Check size={13} /> GitWyrm will ask first
        </span>
      )}
    </SettingRow>
  )
}

/** Master switch for every Specs surface: the toolbar button and the sidebar section. */
function SpecDeskSetting() {
  const enabled = useWorkspaceStore((s) => s.enableSpecDesk)
  const setEnabled = useWorkspaceStore((s) => s.setEnableSpecDesk)

  return (
    <SettingRow
      label="Spec Desk"
      searchId="spec-desk"
      hint="Plan work as written specs alongside your code. Shows the Spec Desk button and the Specs list in the sidebar, even before a repository has an openspec/ folder, so you can start one."
    >
      <label className="flex cursor-pointer items-center gap-2 text-xs text-foreground">
        <input
          type="checkbox"
          checked={enabled}
          onChange={(e) => setEnabled(e.target.checked)}
          className="size-3.5 accent-[var(--gw-accent)]"
        />
        Show Spec Desk
      </label>
    </SettingRow>
  )
}

/** Editable archive-commit message template with a reset-to-default control. */
function ArchiveCommitTemplateSetting() {
  const defaultQuery = useOpenspecDefaultArchiveTemplate()
  const template = useWorkspaceStore((s) => s.openspecArchiveCommitTemplate)
  const setTemplate = useWorkspaceStore((s) => s.setOpenspecArchiveCommitTemplate)

  const defaultText = defaultQuery.data ?? ''
  // Local draft so typing is smooth; committed to the store on blur.
  const [draft, setDraft] = useState<string | null>(null)
  const [saved, setSaved] = useState(false)
  const reveal = useRevealHighlight('openspec-archive-commit-template')
  const savedTimer = useRef<ReturnType<typeof setTimeout> | null>(null)
  const value = draft ?? template ?? defaultText
  const isCustom = template != null && template.trim() !== ''

  const commit = (text: string) => {
    const trimmed = text.trim()
    // Treat "same as default" or empty as "use default" (null).
    setTemplate(trimmed === '' || trimmed === defaultText.trim() ? null : text)
    setSaved(true)
    if (savedTimer.current) clearTimeout(savedTimer.current)
    savedTimer.current = setTimeout(() => setSaved(false), 1600)
  }

  const reset = () => {
    setDraft(defaultText)
    setTemplate(null)
    setSaved(true)
    if (savedTimer.current) clearTimeout(savedTimer.current)
    savedTimer.current = setTimeout(() => setSaved(false), 1600)
  }

  useEffect(
    () => () => {
      if (savedTimer.current) clearTimeout(savedTimer.current)
    },
    []
  )

  return (
    <div ref={reveal.ref} data-settings-row className={settingRowClass(reveal.flash)}>
      <div className="w-52 flex-none">
        <div className="flex items-center gap-2">
          <span className="text-xs font-semibold text-foreground">Archive commit message</span>
          {isCustom && (
            <span className="rounded bg-panel3 px-1.5 py-0.5 text-2xs font-medium text-sub">
              Customized
            </span>
          )}
        </div>
        <div className="mt-0.5 text-2xs text-muted-foreground">
          Used automatically when you archive a spec. No AI involved.{' '}
          <code className="rounded bg-panel3 px-1 py-0.5">{'{id}'}</code> is replaced with the
          spec's name.
        </div>
      </div>
      <div className="min-w-0 flex-1 space-y-2">
        <Input
          value={value}
          onChange={(e) => setDraft(e.target.value)}
          onBlur={() => draft != null && commit(draft)}
          onKeyDown={(e) => e.key === 'Enter' && draft != null && commit(draft)}
          spellCheck={false}
          className="h-8 bg-background font-mono text-xs"
          placeholder={defaultText}
        />
        <div className="flex items-center gap-2">
          <Button
            variant="ghost"
            size="sm"
            className="h-7 px-2 text-xs text-muted-foreground hover:text-foreground"
            disabled={!isCustom || defaultText === ''}
            onClick={reset}
          >
            <RotateCcw size={12} />
            Reset to default
          </Button>
          {saved && (
            <span className="flex animate-in items-center gap-1 text-2xs text-accent-text fade-in slide-in-from-left-1">
              <Check size={12} /> Saved
            </span>
          )}
        </div>
      </div>
    </div>
  )
}
