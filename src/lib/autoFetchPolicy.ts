/**
 * Scheduling and back-off rules for background fetching.
 *
 * Split out of `useAutoFetch` so the rules can be tested without a DOM: the
 * hook imports the workspace store, which touches `window` at module load and
 * cannot be imported into a plain unit test.
 */

/** How often the repo you are looking at is fetched. */
export const FOREGROUND_FETCH_INTERVAL_MS = 3 * 60_000

/**
 * How often each background tab is fetched. Deliberately much less frequent
 * than the foreground: a tab you are not looking at only needs to be roughly
 * current by the time you click it.
 */
export const BACKGROUND_FETCH_INTERVAL_MS = 15 * 60_000

/**
 * Preferred gap between background tabs in one sweep. Ten open repos firing
 * `git fetch` at the same instant is ten processes and ten network connections
 * at once, which stalls the machine and can trip host rate limits.
 *
 * This is an upper bound, not a guarantee -- see `staggerFor`.
 */
export const BACKGROUND_FETCH_STAGGER_MS = 20_000

/**
 * Share of the interval a sweep is allowed to occupy. The remainder is
 * headroom, so the last repo's fetch has time to finish before the next sweep
 * begins.
 */
export const SWEEP_BUDGET = 0.8

/**
 * Gap to use for a sweep of `count` repos.
 *
 * At the preferred 20s gap a sweep only fits about 36 repos inside its 15
 * minute interval. Beyond that a fixed gap would schedule the tail past the
 * next sweep, so sweeps would overlap and pile up: the repos at the end of the
 * list would be re-queued before they were ever reached, and would never be
 * fetched at all while the ones at the front were fetched repeatedly.
 *
 * Compressing the gap keeps every sweep inside its own interval no matter how
 * many tabs are open. With very many tabs the fetches do land closer together
 * than we would like -- but `fetchIfDue` still bounds each repo to one fetch
 * per interval, and every repo gets its turn, which matters more.
 */
export function staggerFor(count: number): number {
  if (count <= 1) return BACKGROUND_FETCH_STAGGER_MS
  const budget = BACKGROUND_FETCH_INTERVAL_MS * SWEEP_BUDGET
  return Math.min(BACKGROUND_FETCH_STAGGER_MS, Math.floor(budget / (count - 1)))
}

/**
 * Whether an error means "this credential will not work here", as opposed to a
 * transient failure worth retrying.
 *
 * Retrying an unauthorised remote is not merely wasted work: every attempt runs
 * the credential helper, and on a remote the credential cannot satisfy -- an
 * org behind SAML or OAuth app restrictions -- the helper has nothing to do but
 * ask the user again. A field log showed exactly that: one lapsed org
 * authorization turning into a fetch failure every ~20 seconds across 22 open
 * repositories, which is what "spammed with login prompts" looked like from
 * inside the app.
 *
 * Matched on the message because that is all the command returns. Kept narrow
 * deliberately -- treating an ordinary network blip as an auth failure would
 * silently stop fetching a perfectly good repo for the rest of the session,
 * which is far worse than one extra retry.
 */
export function isAuthFailure(message: string): boolean {
  const m = message.toLowerCase()
  return (
    m.includes('sign-in needed') ||
    m.includes('could not read username') ||
    m.includes('could not read password') ||
    m.includes('authentication failed') ||
    m.includes('oauth app access restrictions') ||
    m.includes('saml enforcement') ||
    // Only the parenthesised form git and gh actually print. A bare "403" would
    // match a branch or path that merely contains those digits.
    m.includes('(http 403)') ||
    m.includes('403 forbidden')
  )
}
