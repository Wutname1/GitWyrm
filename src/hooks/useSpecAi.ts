import { useAiCatalog, useAiModels } from '@/hooks/useAi'
import { useAiSelection } from '@/hooks/useAiSelection'

/**
 * Whether the Specs surfaces can run AI, and what to say about it.
 *
 * Three states, deliberately distinct:
 *  - `none`: no provider chosen. AI entry points are hidden, not greyed.
 *  - `reconnect`: a provider is chosen and credentials exist, but the provider
 *    reports no usable models. GitWyrm has a known failure mode here -- a stale
 *    Copilot token still looks signed in while returning zero enabled models --
 *    so treating "credentials present" as "ready" would offer a run that cannot
 *    start.
 *  - `ready`: a provider, a model, and a live list confirming real entitlements.
 */
export type SpecAiState = 'none' | 'reconnect' | 'ready'

export interface SpecAi {
  state: SpecAiState
  /** True only in `ready`. The single gate every AI entry point checks. */
  configured: boolean
  /** Display name, e.g. "GitHub Copilot". Empty when nothing is chosen. */
  provider: string
  /** Short form for sentences: "Copilot" rather than "GitHub Copilot". */
  providerShort: string
  /** Chosen model's display name, empty when unknown. */
  model: string
}

/** "GitHub Copilot" reads as "Copilot" mid-sentence. */
function shorten(name: string): string {
  return name.replace(/^GitHub\s+/i, '')
}

export function useSpecAi(): SpecAi {
  // The provider/model/credentials rule is shared with commit generation via
  // the resolver. Only the model-entitlement check below is specific to Specs,
  // which offers to start a run and so must not offer one that cannot start.
  const { provider: providerId, model: modelId, configured } = useAiSelection()
  const catalog = useAiCatalog()

  const hasCredentials = providerId != null && configured.includes(providerId)

  // Only ask for models once credentials exist -- the call is the verification
  // step, and there is nothing to verify without them.
  const models = useAiModels(hasCredentials ? providerId : null, hasCredentials)

  const providerName =
    (catalog.data ?? []).find((p) => p.id === providerId)?.name ?? providerId ?? ''
  const modelName =
    (models.data?.models ?? []).find((m) => m.id === modelId)?.name ?? modelId ?? ''

  const base = {
    provider: providerName,
    providerShort: shorten(providerName),
    model: modelName,
  }

  if (!providerId || !hasCredentials) {
    return { state: 'none', configured: false, ...base }
  }

  // While the model list is still loading, say nothing rather than flashing a
  // reconnect warning that resolves a moment later.
  if (models.isLoading || models.data == null) {
    return { state: 'none', configured: false, ...base }
  }

  const usable = models.data.models.filter((m) => m.enabled)
  // A live list with nothing enabled is the stale-token signature. A non-live
  // list is a static fallback and cannot prove anything either way, so trust the
  // credentials in that case rather than crying wolf.
  if (models.data.live && usable.length === 0) {
    return { state: 'reconnect', configured: false, ...base }
  }

  return { state: 'ready', configured: true, ...base }
}
