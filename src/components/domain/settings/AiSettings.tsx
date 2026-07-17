import { useMemo, useState } from 'react'
import { Check, Trash2 } from 'lucide-react'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { useAiCatalog, useAiConfigured, useAiMutations, useCopilotSignIn } from '@/hooks/useAi'
import { useWorkspaceStore } from '@/stores/workspaceStore'

const selectClass =
  'h-8 w-full rounded-md border border-input bg-background px-2 text-xs text-foreground outline-none focus:border-ring'

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
  const configuredIds = useMemo(
    () => new Set((configured.data ?? []).map((c) => c.id)),
    [configured.data]
  )
  const isConfigured = aiProvider != null && configuredIds.has(aiProvider)

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
            {providers.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
                {configuredIds.has(p.id) ? ' ✓' : ''}
              </option>
            ))}
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
              Used to generate commit messages from your staged changes.
            </div>
          </div>
          <div className="min-w-0 flex-1">
            <select
              className={selectClass}
              value={aiModel ?? ''}
              onChange={(e) => setAiSelection(provider.id, e.target.value || null)}
            >
              {provider.models.map((mo) => (
                <option key={mo.id} value={mo.id}>
                  {mo.name}
                </option>
              ))}
            </select>
          </div>
        </div>
      )}
    </div>
  )
}
