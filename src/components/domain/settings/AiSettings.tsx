import { useEffect, useMemo, useRef, useState } from 'react'
import { Check, RotateCcw, Trash2 } from 'lucide-react'
import { toast } from 'sonner'
import { DeviceCodePanel } from '@/components/domain/github/DeviceCodePanel'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Textarea } from '@/components/ui/textarea'
import { PendingIndicator } from '@/components/ui/pending-indicator'
import {
  useAiCatalog,
  useAiConfigured,
  useAiDefaultInstruction,
  useAiModels,
  useAiMutations,
  useCopilotSignIn,
} from '@/hooks/useAi'
import { useWorkspaceStore } from '@/stores/workspaceStore'

const selectClass =
  'h-8 w-full rounded-md border border-input bg-background px-2 text-xs text-foreground outline-none focus:border-ring'

/** Curated providers surfaced at the top of the picker, in display order. */
const POPULAR_PROVIDER_IDS = [
  'github-copilot',
  'anthropic',
  'openai',
  'google',
  'openrouter',
  'deepseek',
] as const

export function AiSettings() {
  const catalog = useAiCatalog()
  const configured = useAiConfigured()
  const m = useAiMutations()
  const aiProvider = useWorkspaceStore((s) => s.aiProvider)
  const aiModel = useWorkspaceStore((s) => s.aiModel)
  const setAiSelection = useWorkspaceStore((s) => s.setAiSelection)

  const [keyDraft, setKeyDraft] = useState('')
  const copilot = useCopilotSignIn()

  const providers = catalog.data ?? []
  const provider = providers.find((p) => p.id === aiProvider) ?? null

  // Split the catalog into a curated "Popular" group (fixed order, Copilot
  // first) and the alphabetical long tail of everything else.
  const { popular, rest } = useMemo(() => {
    const byId = new Map(providers.map((p) => [p.id, p]))
    const popular = POPULAR_PROVIDER_IDS.map((id) => byId.get(id)).filter(
      (p): p is (typeof providers)[number] => p != null
    )
    const popularIds = new Set<string>(POPULAR_PROVIDER_IDS)
    const rest = providers.filter((p) => !popularIds.has(p.id))
    return { popular, rest }
  }, [providers])
  const configuredIds = useMemo(
    () => new Set((configured.data ?? []).map((c) => c.id)),
    [configured.data]
  )
  const isConfigured = aiProvider != null && configuredIds.has(aiProvider)

  // Credentials without a selection reads as "not set up". When there's only
  // one configured provider the intent is unambiguous, so adopt it. The model
  // is left null for the repair effect below to fill from the live list.
  useEffect(() => {
    if (aiProvider != null || configured.data == null) return
    if (configured.data.length !== 1) return
    setAiSelection(configured.data[0].id, null)
  }, [aiProvider, configured.data, setAiSelection])

  const modelsQuery = useAiModels(aiProvider, isConfigured)
  const models = modelsQuery.data?.models ?? provider?.models ?? []
  // The static catalog marks every model enabled because it cannot know plan
  // entitlements. Only a live list is evidence a model is actually usable.
  const entitlementsKnown = modelsQuery.data?.live ?? false

  // Keep the persisted model valid and usable: if the saved model isn't in the
  // live list or is disabled (e.g. Copilot plan-gated), fall back to the first
  // enabled one. Leaves the selection alone when only disabled models exist.
  //
  // Only ever auto-selects from a live list. Picking off the static list would
  // hand the user a model their plan may not include, which then fails at
  // generate time far from the cause.
  useEffect(() => {
    if (!provider || models.length === 0 || !entitlementsKnown) return
    const saved = models.find((mo) => mo.id === aiModel)
    if (saved?.enabled) return
    const firstEnabled = models.find((mo) => mo.enabled)
    if (firstEnabled && firstEnabled.id !== aiModel) {
      setAiSelection(provider.id, firstEnabled.id)
    }
  }, [provider, models, aiModel, entitlementsKnown, setAiSelection])

  const saveKey = () => {
    if (!aiProvider || !keyDraft.trim()) return
    m.setApiKey.mutate(
      { provider: aiProvider, key: keyDraft },
      {
        onSuccess: () => {
          setKeyDraft('')
          toast.success('API key saved')
        },
        onError: (e) => toast.error(String(e)),
      }
    )
  }

  if (catalog.isLoading) {
    return <div className="py-3 text-xs text-muted-foreground">Loading provider catalog…</div>
  }
  if (catalog.isError) {
    return (
      <div className="grid justify-items-start gap-2 py-3">
        <div className="text-xs text-destructive">
          Could not load the list of AI providers. This needs a connection the first time.
        </div>
        <Button
          variant="secondary"
          size="sm"
          className="h-8 text-xs"
          disabled={catalog.isFetching}
          onClick={() => void catalog.refetch()}
        >
          {catalog.isFetching ? <PendingIndicator /> : <RotateCcw size={12} />}
          {catalog.isFetching ? 'Retrying…' : 'Try again'}
        </Button>
      </div>
    )
  }

  return (
    <div className="space-y-0">
      <div className="flex items-start gap-6 py-3">
        <div className="w-52 flex-none">
          <div className="text-xs font-semibold text-foreground">Provider</div>
          <div className="mt-0.5 text-2xs text-muted-foreground">
            Bring your own API key. Keys are stored locally and never leave this machine except
            to call the provider.
          </div>
        </div>
        <div className="min-w-0 flex-1 space-y-2">
          <select
            className={selectClass}
            value={aiProvider ?? ''}
            onChange={(e) => {
              // Leave the model unset: the effect above fills it once the live
              // list confirms what this account can actually use.
              setAiSelection(e.target.value || null, null)
              setKeyDraft('')
            }}
          >
            <option value="">Select a provider…</option>
            <optgroup label="Popular">
              {popular.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name}
                  {configuredIds.has(p.id) ? ' ✓' : ''}
                </option>
              ))}
            </optgroup>
            <optgroup label="All providers">
              {rest.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name}
                  {configuredIds.has(p.id) ? ' ✓' : ''}
                </option>
              ))}
            </optgroup>
          </select>

          {provider && (
            <>
              {isConfigured ? (
                <div className="flex items-center gap-2">
                  <span className="flex items-center gap-1.5 text-xs text-sub">
                    <Check size={13} className="text-green-500" />
                    API key configured
                  </span>
                  <Button
                    variant="ghost"
                    size="sm"
                    className="h-7 px-2 text-xs text-muted-foreground hover:text-destructive"
                    tooltip="Remove API key"
                    disabled={m.removeProvider.isPending}
                    aria-busy={m.removeProvider.isPending || undefined}
                    onClick={() =>
                      m.removeProvider.mutate(provider.id, {
                        onError: (e) => toast.error(String(e)),
                      })
                    }
                  >
                    {m.removeProvider.isPending ? <PendingIndicator /> : <Trash2 size={12} />}
                    {m.removeProvider.isPending ? 'Removing…' : 'Remove'}
                  </Button>
                </div>
              ) : provider.id === 'github-copilot' ? (
                <div className="space-y-2">
                  {copilot.status.state === 'waiting' ? (
                    <DeviceCodePanel
                      userCode={copilot.status.userCode}
                      verificationUri={copilot.status.verificationUri}
                      onCancel={copilot.cancel}
                    />
                  ) : (
                    <Button
                      variant="secondary"
                      size="sm"
                      className="h-8 text-xs"
                      disabled={copilot.status.state === 'starting'}
                      onClick={copilot.start}
                    >
                      {copilot.status.state === 'starting'
                        ? 'Starting sign-in…'
                        : 'Sign in with GitHub'}
                    </Button>
                  )}
                  {copilot.status.state === 'error' && (
                    <div className="text-2xs text-destructive">{copilot.status.message}</div>
                  )}
                </div>
              ) : (
                <div className="flex gap-2">
                  <Input
                    type="password"
                    value={keyDraft}
                    onChange={(e) => setKeyDraft(e.target.value)}
                    onKeyDown={(e) => e.key === 'Enter' && saveKey()}
                    placeholder={`${provider.name} API key`}
                    className="h-8 bg-background font-mono text-xs"
                    autoComplete="off"
                  />
                  <Button
                    variant="secondary"
                    size="sm"
                    className="h-8 flex-none"
                    disabled={!keyDraft.trim() || m.setApiKey.isPending}
                    onClick={saveKey}
                  >
                    {m.setApiKey.isPending && <PendingIndicator />}
                    {m.setApiKey.isPending ? 'Saving…' : 'Save'}
                  </Button>
                </div>
              )}
            </>
          )}
        </div>
      </div>

      {provider && (
        <div className="flex items-start gap-6 py-3">
          <div className="w-52 flex-none">
            <div className="text-xs font-semibold text-foreground">Model</div>
            <div className="mt-0.5 text-2xs text-muted-foreground">
              {isConfigured
                ? 'Shows the models your account can use.'
                : 'Used to generate commit messages from your staged changes.'}
            </div>
          </div>
          <div className="min-w-0 flex-1">
            <select
              className={selectClass}
              value={aiModel ?? ''}
              disabled={modelsQuery.isLoading}
              onChange={(e) => setAiSelection(provider.id, e.target.value || null)}
            >
              <option value="">Select a model…</option>
              {models.map((mo) => (
                <option key={mo.id} value={mo.id} disabled={!mo.enabled}>
                  {mo.name}
                  {mo.enabled ? '' : ' - needs an active Copilot subscription'}
                </option>
              ))}
            </select>
            {modelsQuery.isFetching ? (
              <div className="mt-1 text-2xs text-muted-foreground">Loading your models…</div>
            ) : (
              isConfigured &&
              !entitlementsKnown && (
                <div className="mt-1 text-2xs text-muted-foreground">
                  We could not check which models your account can use, so nothing is picked for
                  you. Choose one, or retry the connection.
                </div>
              )
            )}
          </div>
        </div>
      )}

      <InstructionSetting />
    </div>
  )
}

/** Editable system instruction with a reset-to-default control. */
function InstructionSetting() {
  const defaultQuery = useAiDefaultInstruction()
  const aiInstruction = useWorkspaceStore((s) => s.aiInstruction)
  const setAiInstruction = useWorkspaceStore((s) => s.setAiInstruction)

  const defaultText = defaultQuery.data ?? ''
  // Local draft so typing is smooth; committed to the store on blur.
  const [draft, setDraft] = useState<string | null>(null)
  const [saved, setSaved] = useState(false)
  const savedTimer = useRef<ReturnType<typeof setTimeout> | null>(null)
  const value = draft ?? aiInstruction ?? defaultText
  const isCustom = aiInstruction != null && aiInstruction.trim() !== ''

  const commit = (text: string) => {
    const trimmed = text.trim()
    // Treat "same as default" or empty as "use default" (null).
    setAiInstruction(trimmed === '' || trimmed === defaultText.trim() ? null : text)
    setSaved(true)
    if (savedTimer.current) clearTimeout(savedTimer.current)
    savedTimer.current = setTimeout(() => setSaved(false), 1600)
  }

  const reset = () => {
    setDraft(defaultText)
    setAiInstruction(null)
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
    <div className="flex items-start gap-6 py-3">
      <div className="w-52 flex-none">
        <div className="flex items-center gap-2">
          <span className="text-xs font-semibold text-foreground">Instructions</span>
          {isCustom && (
            <span className="rounded bg-panel3 px-1.5 py-0.5 text-2xs font-medium text-sub">
              Customized
            </span>
          )}
        </div>
        <div className="mt-0.5 text-2xs text-muted-foreground">
          Guidance for tone and style. GitWyrm always enforces the summary and
          description format on top of this.
        </div>
      </div>
      <div className="min-w-0 flex-1 space-y-2">
        <Textarea
          value={value}
          onChange={(e) => setDraft(e.target.value)}
          onBlur={() => draft != null && commit(draft)}
          rows={8}
          spellCheck={false}
          className="resize-y bg-background px-2.5 py-2 font-mono text-2xs leading-relaxed"
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
