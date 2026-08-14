import { Star } from 'lucide-react'
import { cn } from '@/lib/utils'
import type { CatalogProvider } from '@/lib/bindings'
import { useAiModels } from '@/hooks/useAi'
import { useWorkspaceStore } from '@/stores/workspaceStore'
import { settingRowClass } from './SettingRow'

/**
 * Model choice for one configured provider.
 *
 * Fetches that provider's own list rather than reusing the selected provider's,
 * so a row shows what that account can actually run -- a Copilot plan gates
 * models per account, and offering a disabled one fails later at generate time,
 * far from the cause.
 */
function ProviderModelPicker({
  provider,
  model,
}: {
  provider: CatalogProvider
  model: string | null
}) {
  const setAiModelFor = useWorkspaceStore((s) => s.setAiModelFor)
  const query = useAiModels(provider.id, true)
  const models = query.data?.models ?? provider.models

  return (
    <select
      aria-label={`Model for ${provider.name}`}
      className="h-7 w-44 flex-none rounded-md border border-input bg-background px-1.5 text-2xs text-foreground outline-none focus:border-ring"
      value={model ?? ''}
      onChange={(e) => setAiModelFor(provider.id, e.target.value || null)}
    >
      {model == null && <option value="">Choose a model…</option>}
      {models.map((mo) => (
        <option key={mo.id} value={mo.id} disabled={!mo.enabled}>
          {mo.name}
          {mo.enabled ? '' : ' (not in your plan)'}
        </option>
      ))}
    </select>
  )
}

/**
 * The providers that are set up, and which one everything uses.
 *
 * Only shown once a second provider is configured. With one provider the default
 * is unambiguous and a list of one row asking the user to choose is noise -- the
 * setting should not need to be understood by someone who only has Copilot.
 */
export function AiDefaultProviders({
  providers,
  configuredIds,
}: {
  providers: CatalogProvider[]
  configuredIds: Set<string>
}) {
  const aiProvider = useWorkspaceStore((s) => s.aiProvider)
  const aiModels = useWorkspaceStore((s) => s.aiModels)
  const setDefaultAiProvider = useWorkspaceStore((s) => s.setDefaultAiProvider)

  const configured = providers.filter((p) => configuredIds.has(p.id))
  if (configured.length < 2) return null

  return (
    <div data-settings-row className={settingRowClass(false)}>
      <div className="w-52 flex-none">
        <div className="text-xs font-semibold text-foreground">Default</div>
        <div className="mt-0.5 text-2xs text-muted-foreground">
          Choose which connected provider GitWyrm uses first.
        </div>
      </div>
      <div className="min-w-0 flex-1 space-y-1">
        {configured.map((p) => {
          const isDefault = p.id === aiProvider
          return (
            <div
              key={p.id}
              className={cn(
                'flex items-center gap-2.5 rounded-md border px-2.5 py-2 transition-colors',
                isDefault ? 'border-primary/40 bg-soft' : 'border-border'
              )}
            >
              {isDefault ? (
                <button
                  type="button"
                  disabled
                  aria-pressed
                  className="flex flex-none items-center gap-1.5 text-2xs font-semibold text-accent-text"
                >
                  <Star size={12} strokeWidth={2.4} className="fill-current" />
                  Default
                </button>
              ) : (
                <button
                  type="button"
                  onClick={() => setDefaultAiProvider(p.id)}
                  aria-pressed={false}
                  className="flex-none rounded px-1.5 py-0.5 text-2xs text-muted-foreground transition-colors hover:bg-panel2 hover:text-foreground"
                >
                  Make default
                </button>
              )}
              <span className="min-w-0 flex-1 truncate text-xs font-medium text-foreground">
                {p.name}
              </span>
              <ProviderModelPicker provider={p} model={aiModels[p.id] ?? null} />
            </div>
          )
        })}
      </div>
    </div>
  )
}
