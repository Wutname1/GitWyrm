import { useAiConfigured } from '@/hooks/useAi'
import { useWorkspaceStore } from '@/stores/workspaceStore'

/**
 * Which provider and model an AI feature should use.
 *
 * Every AI feature reads this rather than the raw setting. Before it existed,
 * the commit-message form, the commit-generation dialog, the AI settings view,
 * and the Spec Desk each re-derived "is a provider actually usable?" from the
 * store plus the configured list -- four copies of one rule, free to disagree.
 *
 * `ready` means: a provider is the default, a model is chosen for it, and its
 * credentials are actually on disk. A provider whose credentials were removed
 * elsewhere is not ready, however recently it was the default.
 */
export interface AiSelection {
  /** True only when this selection can be used to make a request. */
  ready: boolean
  /** Default provider id, or null when none is set. */
  provider: string | null
  /** Model for that provider, or null when none is chosen. */
  model: string | null
  /** Provider ids with credentials stored, in id order. */
  configured: string[]
  /**
   * Whether the user has AI switched on.
   *
   * Separate from `ready` so the chip and the settings view can say "off" while
   * every feature gate sees the same not-ready as it would with no provider at
   * all. Only surfaces that talk *about* the AI need this; anything deciding
   * whether it can make a request wants `ready`.
   */
  enabled: boolean
}

export function useAiSelection(): AiSelection {
  const provider = useWorkspaceStore((s) => s.aiProvider)
  const model = useWorkspaceStore((s) => s.aiModel)
  const enabled = useWorkspaceStore((s) => s.aiEnabled)
  const configuredQuery = useAiConfigured()

  const configured = (configuredQuery.data ?? [])
    .filter((c) => c.configured)
    .map((c) => c.id)

  // Switched off is not-ready by exactly the same route as unconfigured, so no
  // feature needs its own opinion about the difference.
  const ready =
    enabled && provider != null && model != null && configured.includes(provider)

  return { ready, provider, model, configured, enabled }
}

/**
 * The provider that should become default when the current one goes away.
 *
 * Picking the first configured id keeps this deterministic -- the list arrives
 * in id order -- so removing a provider does not land the user somewhere that
 * looks arbitrary. Null when nothing is left, which every AI feature already
 * treats as "not set up".
 */
export function nextDefaultProvider(
  configured: string[],
  removing: string
): string | null {
  return configured.find((id) => id !== removing) ?? null
}
