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

## Terms of service: researched, and it changes the design

This was the spike's one open question. It is now answered for Anthropic, and the answer
is no.

### Anthropic: prohibited, explicitly and by name

Anthropic's [Claude Code legal page](https://code.claude.com/docs/en/legal-and-compliance)
addresses this exact design:

> OAuth authentication is intended exclusively for purchasers of Claude Free, Pro, Max,
> Team, and Enterprise subscription plans and is designed to support ordinary use of
> Claude Code and other native Anthropic applications. Developers building products or
> services that interact with Claude's capabilities, including those using the Agent SDK,
> should use API key authentication through Claude Console or a supported cloud provider.
> **Anthropic does not permit third-party developers to offer Claude.ai login or to route
> requests through Free, Pro, or Max plan credentials on behalf of their users.**
> Anthropic reserves the right to take measures to enforce these restrictions and may do
> so without prior notice.

The [Consumer Terms](https://www.anthropic.com/legal/consumer-terms) (eff. 2025-10-08)
also prohibit automated access "except when you are accessing our Services via an
Anthropic API Key or where we otherwise explicitly permit it".

Two traps worth naming, because both were tempting:

1. **The subprocess mechanism is sanctioned; the credential is not.** Anthropic documents
   running `claude -p --output-format json` as a subprocess for exactly this purpose. That
   made it look permitted. It is not the same question: invocation is fine, doing it
   against a subscription on a user's behalf is prohibited. The two are answered
   separately in their docs.
2. **"The user runs it on their own machine with their own credentials" does not rescue
   it.** The Consumer Terms prohibit helping *another person* access the service by
   automated means, and the legal page names routing "on behalf of their users". Shipping
   the feature is the violation regardless of whose machine executes it.

Reporting indicates third-party OAuth access began being blocked in January 2026 with the
docs clarification following in February. So the risk is not only legal: the integration
would break unannounced and users would blame GitWyrm.

**Also a naming constraint.** The Agent SDK terms forbid calling a feature "Claude Code"
or using Claude Code-branded visuals. "Powered by Claude" is the ceiling. Nothing in
GitWyrm should advertise "Claude Code integration".

### What this means for the engine

- **Anthropic access is BYO API key only.** The user pastes a key from Claude Console and
  is billed for it. This is squarely permitted and is what the existing BYO-AI settings
  already do.
- **No subscription-credential path for Anthropic**, and specifically no fallback that
  reaches for subscription auth when a key is absent. A path that shells out to `claude`
  only when `ANTHROPIC_API_KEY` is set bills the API rather than the subscription and is
  defensible; one that falls back to the subscription is the prohibited pattern.
- If subscription reuse ever becomes strategically necessary, the only legitimate route is
  written approval from Anthropic ("unless previously approved"), via contact sales. That
  is a decision to seek, not an assumption to build on.

### OpenAI / Codex: silence, which is not permission

Also researched. The finding is different from Anthropic's but lands in the same place.

The mechanism is blessed: `codex exec` is documented for non-interactive use, and OpenAI
even documents an **App Server** protocol for "embed Codex into your product". The
problem is the auth path, and two things about it:

1. OpenAI's Terms of Use (published and effective **January 1, 2026**, confirmed verbatim
   from archived captures of both `/policies/terms-of-use/` and `/policies/row-terms-of-use/`)
   list under "What you cannot do": "**Automatically or programmatically extract data or
   Output**". Read plainly, that covers shelling out to `codex exec` and parsing stdout.
   Arguably it targets bulk scraping rather than this - but the plain text reaches it.
   The same list also forbids "Modify, copy, lease, sell or distribute any of our
   Services", and the Business Terms (eff. 2026-01-01) add §3.1 "Customer may not resell
   or lease access to its Account or any End User Account" and §3.3(f) "extract data from
   the Services other than as permitted through the Services".
2. OpenAI's own docs steer programmatic use to API keys: "Use API key authentication for
   programmatic Codex CLI workflows". Account-based auth in automation is documented for
   "trusted scripts, schedulers, and private CI runners", explicitly *not* for public
   distribution.

The decisive evidence is not a clause, it is a pattern. In
[openai/codex Discussion #8338](https://github.com/openai/codex/discussions/8338),
developers asked this exact question four times over seven months - one describing a
separate commercial app using the subscription as its auth layer, almost precisely this
design. OpenAI's only substantive reply was an engineer saying he is "not qualified",
that the terms are "quite permissive", that "OSS projects like OpenCode are doing things
similar", and to consult a lawyer. Two later asks got no reply at all.

Four opportunities to say "yes, this is fine", and no. That silence is the finding.

Note also that Codex CLI's Apache-2.0 licence grants rights to the *client code only*. A
licence to fork a CLI is not a licence to use the service behind it; only the ToS governs
the calls.

### Precedent: two patterns, only one of which drew enforcement

Surveying what other projects actually do (verified through the GitHub API, not web
search) separates the risk into two patterns that are easy to conflate:

**Pattern A - drive the user's own installed CLI as a subprocess.** Spawn `claude`,
`codex`, or `copilot`, talk over stdin/stdout in JSON mode, and let the CLI authenticate
itself. GitWyrm never sees a token. This is what every healthy multi-agent tool does:
`BloopAI/vibe-kanban` (27.6k stars), `smtg-ai/claude-squad` (8.2k), `stravu/crystal`
(3.1k), `kbwo/ccmanager` (1.2k, which drives eight different CLIs), and GitHub's own
`github/copilot-cli`. A code search for the headless flag `claude -p --output-format
stream-json` returns 30 repositories. **No enforcement against this pattern was found.**

**Pattern B - read the CLI's stored credentials and call the API yourself.** Read
`~/.claude/.credentials.json` or the `Claude Code-credentials` keychain entry, then make
your own authenticated requests. This is the pattern that drew action:

- `anomalyco/opencode` PR
  [#18186](https://github.com/anomalyco/opencode/pull/18186), titled literally
  **"anthropic legal requests"**, authored by opencode's lead and merged 2026-03-19:
  "Remove anthropic references per legal requests... Remove opencode-anthropic-auth
  builtin plugin". The plugin's repository now returns 404. *Verified directly via the
  GitHub API, not reported second-hand.*
- opencode issue #6930 (2026-01-05), "Using opencode with Anthropic OAuth violates ToS &
  Results in Ban" - a user reporting an actual account ban.
- A trail of merged PRs in other projects retreating to API keys for "TOS compliance",
  one of which explicitly moved to "API key **or local Claude Code CLI only**" - that is,
  it retreated from Pattern B *to Pattern A*.
- The capability survives only in third-party plugins carrying disclaimers like "You
  might be banned for breaking the TOS, you might not be", one of which works by
  rewriting the system prompt to impersonate Claude Code's identity.

One counter-datapoint, recorded for honesty: `mastra-ai/mastra` later removed its own ToS
warning, saying users were not in fact being banned. Enforcement is inconsistent in
practice even where the legal position is not.

### Where this leaves each provider

The precedent refines the conclusion rather than overturning it, and the two patterns
deserve different answers.

**Pattern B is out, for every provider.** Reading another application's credential files
is what Anthropic's lawyers acted on, and GitWyrm should not do it regardless of provider.
It is already a spec requirement below.

**Pattern A remains genuinely open for Anthropic** - and this is where the terms and the
precedent disagree. Anthropic's legal page prohibits routing requests "through Free, Pro,
or Max plan credentials on behalf of their users", which on its face covers a subprocess
too. But no enforcement against subprocess-driving has been found, GitHub's own Copilot
CLI is built for it, and Anthropic documents `claude -p --output-format json` for exactly
this purpose. The distinction that may matter is *who authenticates*: in Pattern A the
user's own installed CLI does, using its own sign-in, and GitWyrm is closer to a terminal
than to a client.

That is a judgement call, not a settled question, and it is the product owner's to make.
It should not be made by quietly shipping it.

**Recommendation:** build the engine on **BYO API key** as the default and only
out-of-the-box path.

Reasons, in order of weight:

1. It is unambiguously permitted, and is what both providers' own docs recommend for
   programmatic use. Nothing else in the research can be said that plainly.
2. Enforcement lands on *our users* - a suspended account, or an integration that breaks
   unannounced - not on GitWyrm. Users would blame us either way. That makes it a
   product-trust problem as much as a legal one.
3. One transport is one thing to build and test. A subprocess path adds CLI discovery,
   version gating, and per-provider output parsing for a benefit we cannot promise stays.

A Pattern A subprocess path can be added later as a deliberate, documented decision -
ideally after asking the provider. The routes for that are written approval from Anthropic
(contact sales) and OpenAI's App Server known-clients channel. Both are things to obtain,
not to assume.

What we should not do is ship subscription reuse quietly and discover the answer when a
user's account is suspended.

### Still open

Copilot. The question is sharper than it first appeared: GitWyrm already moved off
direct-token access because GitHub gates real model entitlements to an **approved client
allowlist** that GitWyrm's OAuth app is not on, and now routes through GitHub's own
bundled CLI to obtain those entitlements. Whether that is permitted, or is circumvention,
needs the same primary-source treatment. Unlike the other two this already ships, so it
is an existing exposure rather than a design decision.
