import { useEffect, useMemo, useState } from 'react'
import { Check, RotateCcw, Trash2 } from 'lucide-react'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Textarea } from '@/components/ui/textarea'
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

  const modelsQuery = useAiModels(aiProvider, isConfigured)
  const models = modelsQuery.data ?? provider?.models ?? []

  // Keep the persisted model valid and usable: if the saved model isn't in the
  // live list or is disabled (e.g. Copilot plan-gated), fall back to the first
  // enabled one. Leaves the selection alone when only disabled models exist.
  useEffect(() => {
    if (!provider || models.length === 0) return
    const saved = models.find((mo) => mo.id === aiModel)
    if (saved?.enabled) return
    const firstEnabled = models.find((mo) => mo.enabled)
    if (firstEnabled && firstEnabled.id !== aiModel) {
      setAiSelection(provider.id, firstEnabled.id)
    }
  }, [provider, models, aiModel, setAiSelection])

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
      <div className="py-3 text-xs text-destructive">
        Could not load the provider catalog. Check your connection and reopen settings.
      </div>
    )
  }

  return (
    <div className="space-y-0">
      <div className="flex items-start gap-6 py-3">
        <div className="w-52 flex-none">
          <div className="text-xs font-semibold text-foreground">Provider</div>
          <div className="mt-0.5 text-[10.5px] text-muted-foreground">
            Bring your own API key. Keys are stored locally and never leave this machine except
            to call the provider.
          </div>
        </div>
        <div className="min-w-0 flex-1 space-y-2">
          <select
            className={selectClass}
            value={aiProvider ?? ''}
            onChange={(e) => {
              const id = e.target.value || null
              const first = providers.find((p) => p.id === id)?.models[0]?.id ?? null
              setAiSelection(id, first)
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
                    title="Remove API key"
                    onClick={() =>
                      m.removeProvider.mutate(provider.id, {
                        onError: (e) => toast.error(String(e)),
                      })
                    }
                  >
                    <Trash2 size={12} />
                    Remove
                  </Button>
                </div>
              ) : provider.id === 'github-copilot' ? (
                <div className="space-y-2">
                  {copilot.status.state === 'waiting' ? (
                    <div className="rounded-md border border-border bg-background p-2.5 text-xs text-sub">
                      Enter this code on GitHub:{' '}
                      <span className="select-all font-mono text-sm font-bold text-foreground">
                        {copilot.status.userCode}
                      </span>
                      <div className="mt-1.5 flex items-center gap-2">
                        <span className="text-[10.5px] text-muted-foreground">
                          Waiting for authorization…
                        </span>
                        <Button
                          variant="ghost"
                          size="sm"
                          className="h-6 px-2 text-[10.5px]"
                          onClick={copilot.cancel}
                        >
                          Cancel
                        </Button>
                      </div>
                    </div>
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
                    <div className="text-[10.5px] text-destructive">{copilot.status.message}</div>
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
                    Save
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
            <div className="mt-0.5 text-[10.5px] text-muted-foreground">
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
              {models.map((mo) => (
                <option key={mo.id} value={mo.id} disabled={!mo.enabled}>
                  {mo.name}
                  {mo.enabled ? '' : ' — needs an active Copilot subscription'}
                </option>
              ))}
            </select>
            {modelsQuery.isFetching && (
              <div className="mt-1 text-[10.5px] text-muted-foreground">
                Loading your models…
              </div>
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
  const value = draft ?? aiInstruction ?? defaultText
  const isCustom = aiInstruction != null && aiInstruction.trim() !== ''

  const commit = (text: string) => {
    const trimmed = text.trim()
    // Treat "same as default" or empty as "use default" (null).
    setAiInstruction(trimmed === '' || trimmed === defaultText.trim() ? null : text)
  }

  const reset = () => {
    setDraft(defaultText)
    setAiInstruction(null)
  }

  return (
    <div className="flex items-start gap-6 py-3">
      <div className="w-52 flex-none">
        <div className="flex items-center gap-2">
          <span className="text-xs font-semibold text-foreground">Instructions</span>
          {isCustom && (
            <span className="rounded bg-panel3 px-1.5 py-0.5 text-[9.5px] font-medium text-sub">
              Customized
            </span>
          )}
        </div>
        <div className="mt-0.5 text-[10.5px] text-muted-foreground">
          How the AI is told to write your commit messages.
        </div>
      </div>
      <div className="min-w-0 flex-1 space-y-2">
        <Textarea
          value={value}
          onChange={(e) => setDraft(e.target.value)}
          onBlur={() => draft != null && commit(draft)}
          rows={8}
          spellCheck={false}
          className="resize-y bg-background px-2.5 py-2 font-mono text-[11px] leading-relaxed"
          placeholder={defaultText}
        />
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
      </div>
    </div>
  )
}
