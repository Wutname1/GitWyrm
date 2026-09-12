import type { CatalogModel } from '@/lib/bindings'

/**
 * Reading a provider's model list honestly.
 *
 * A list is not automatically evidence about somebody's plan just because it
 * arrived. Copilot's own SDK documents one shape that means the opposite:
 * "list() without a token returns only the `auto` pseudo-model". That reply
 * comes back well-formed and is marked live like any other, so every guard
 * written against an empty or all-disabled list looks straight past it.
 *
 * DESIGN.md puts the general rule this way: do not treat a shaped response as
 * visual proof of a verified one. These helpers are where that judgement lives
 * for model lists, so the pickers and the settings copy cannot disagree about
 * it.
 */

/** Copilot's "let the provider choose" entry, which is a real selection. */
export const AUTO_MODEL_ID = 'auto'

/**
 * Whether the only thing on offer is `auto`.
 *
 * True for the documented no-token reply, and also true for a genuinely
 * minimal account -- the two are indistinguishable from here, which is the
 * point. Callers must not claim to know which one it is; they should stop
 * short of acting as though the list were a measured entitlement.
 */
export function onlyAutoOffered(models: CatalogModel[]): boolean {
  const usable = models.filter((m) => m.enabled)
  return usable.length > 0 && usable.every((m) => m.id === AUTO_MODEL_ID)
}

/**
 * Whether a live list is good enough to pick a model on the user's behalf.
 *
 * An empty list is already refused by the callers. This adds the only-`auto`
 * case, on the grounds that auto-selecting from it overwrites a deliberate
 * choice using a reply GitWyrm cannot vouch for.
 */
export function canAutoSelectFrom(models: CatalogModel[]): boolean {
  return models.some((m) => m.enabled) && !onlyAutoOffered(models)
}

/**
 * What to tell someone about a list, or `null` when it speaks for itself.
 *
 * Deliberately does not assert which cause applies: it names what was seen,
 * says the usual reason, and leaves `auto` standing as a working option, in
 * the same register as the usage card's "not reported".
 */
export function modelListCaveat(models: CatalogModel[]): string | null {
  if (!onlyAutoOffered(models)) return null
  return 'Copilot offered only Auto. That usually means GitWyrm could not read your plan. Auto still works, or reconnect to try again.'
}
