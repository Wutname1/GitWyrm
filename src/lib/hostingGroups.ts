import type { HostProviderInfo } from '@/lib/bindings'

/** The three ways a code host can stand on the Integrations screen. */
export interface HostingGroups {
  /** Signed in and working. */
  connected: HostProviderInfo[]
  /** Set up, but the saved sign-in did not work. */
  failing: HostProviderInfo[]
  /** Nothing saved for this host yet. */
  available: HostProviderInfo[]
}

/**
 * Splits the hosts into connected, needs-attention, and not-set-up.
 *
 * The middle group is the reason this exists. A saved sign-in that fails and a
 * host that was never set up used to look identical, because the backend
 * flattened "the check errored" into "not connected". That put a host you
 * already configured under "Add an integration", which sends you to redo setup
 * when the actual fix is to sign in again.
 *
 * Pulled out of the component so the split is covered by a fast unit test
 * rather than needing a rendered screen -- the same split
 * `src/lib/agentDeskUsage.ts` uses for the same reason.
 */
export function groupHostingProviders(providers: HostProviderInfo[] | undefined): HostingGroups {
  const all = providers ?? []
  return {
    connected: all.filter((p) => p.connected_as != null),
    // Only when nothing is signed in AND a check failed. A host that is
    // connected and also reported a transient error is still connected: it has
    // a working account name, and demoting it would take away the Disconnect
    // button over a blip.
    failing: all.filter((p) => p.connected_as == null && p.auth_error != null),
    available: all.filter((p) => p.connected_as == null && p.auth_error == null),
  }
}
