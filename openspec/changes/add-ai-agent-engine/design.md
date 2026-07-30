# Design

## What the Claude Code CLI spike established

The spike wrapped the local `claude` CLI and generated a properly-formatted commit
message from a real staged diff. Wrapping a CLI-authenticated tool is viable. The
findings below are measured, not estimated, and several of them changed decisions.

### Ask the tool, do not inspect its files

`status()` trusts the CLI's own `loggedIn` field. This is not a style preference: the
spike hit an account whose credential file held a complete OAuth record but whose
sign-in was missing the `user:profile` scope. `claude doctor` reported it plainly. A
file-existence check would have marked that account ready and then failed at generation
time - exactly the stale-credential failure the provider surface exists to prevent.

GitWyrm therefore never reads or writes a provider's credential files. It runs the CLI
and believes what the CLI says about itself.

### Timing, and what it forces

| Input | Measured |
|---|---|
| Trivial prompt | ~5.7 s |
| 334-char diff | ~10.3 s |
| 20 KB diff | ~15.9 s |

Startup is roughly 11% of that; the rest is generation. Consequences:

- **A turn needs cancellable, visible progress.** A 16-second silent spinner reads as a
  hang. This is why the console spec carries a "thinking time is visible" requirement.
- **Streaming is load-bearing, not a nicety.** Time-to-first-token is what makes the wait
  tolerable.
- **Sessions would amortise startup but not fix this** - do not reach for session reuse
  expecting it to solve the wait.

### Numbers not to build on

The envelope reported `total_cost_usd: 0.48` on a Max subscription - probably an
API-equivalent estimate and unverified, certainly not what the user pays. It also
reported `input_tokens: 2` for a 334-character diff, which cannot be counting
system or cached content.

Neither is fit for a budget display or token accounting. GitWyrm shows no prices and no
token counts.

### Version gate, not pinned paths

The CLI auto-updated 2.1.218 → 2.1.220 mid-spike and discovery absorbed it with no code
change. Detect by discovery plus a `--version` floor; do not pin an install path or an
exact build.

## Decisions carried forward

- **Prefer a documented API when the user has a key.** A CLI is not a public interface;
  its output format can change between releases. The CLI path exists so a
  subscription-only user is not shut out. Where a provider API and a key are available,
  use them - this is how opencode reaches providers, and it is the more stable transport.
- **One `ProviderAgent` interface, several implementations.** Claude Code and the Copilot
  CLI sit behind it, as would a direct-API provider. No provider-specific behavior reaches
  the console or any other UI.
- **Guardrails live in GitWyrm's process.** The CLI is a model transport, not a trust
  boundary: never-push, gated side effects, and repo-scoped tools are enforced by our
  code, so they hold no matter what the underlying tool would permit.
- **Cancellation must kill the child process.** A cancelled run cannot leave an orphan
  holding a subscription slot or a half-written file.

## Open question

**Terms of service.** Whether driving a provider's CLI programmatically is permitted
under that provider's terms is unresolved, and the spike could not settle it. It is a
product and legal question, not a technical one, and it applies per provider. This should
be answered before the engine ships to users - a working integration that violates a
provider's terms is worse than no integration.
