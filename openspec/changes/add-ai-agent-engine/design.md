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

### GitHub / Copilot: documented as intended usage

Researched from GitHub's own primary sources. The answer is the opposite of Anthropic's,
and it is the strongest position of the three providers - which is why the Copilot CLI is
the transport that ships first.

**The terms are silent, in the permissive direction.** Individual Copilot use is governed by
Section J (AI Features) of the [GitHub Terms of
Service](https://docs.github.com/en/site-policy/github-terms/github-terms-of-service)
(eff. 2026-04-27), routed there by the [Additional Products
terms](https://docs.github.com/en/site-policy/github-terms/github-terms-for-additional-products-and-features)
(eff. 2026-04-27). Section J covers ownership of Input and Output, training use, and
disclaimers. It says **nothing** about supported clients, editors, programmatic access, or
third-party tools. There is no analogue of Anthropic's "does not permit third-party
developers... to route requests through Free, Pro, or Max plan credentials".

The [Acceptable Use
Policies](https://docs.github.com/en/site-policy/acceptable-use-policies/github-acceptable-use-policies)
restrict scraping - "extracting information from our Service via an automated process, such
as a bot or webcrawler", and explicitly "does not refer to the collection of information
through our API" - and reselling: "You will not reproduce, duplicate, copy, sell, resell or
exploit any portion of the Service... without our express written permission". Neither
reaches a user running an agent on their own machine against their own subscription.

**GitHub documents this exact use affirmatively.** From [About Copilot
CLI](https://docs.github.com/en/copilot/concepts/agents/about-copilot-cli):

> To use the CLI programmatically, include the `-p` or `--prompt` command-line option

> Alternatively, you can use a script to output command-line options and pipe this to
> copilot

> You can use Copilot CLI as an agent in **any third-party tools, IDEs, or automation
> systems** that support this protocol

That last sentence is the whole question, answered by GitHub in its own documentation.
Approval options are documented as allowing "headless operation of the CLI", and the CLI
exposes an Agent Client Protocol server as an integration surface. GitHub's own
`github/copilot-cli` repository (11k stars, active) describes the tool as bringing "the
power of Copilot coding agent directly to your terminal".

**Classification: officially supported.** Not silence, not mere tolerance - documentation.

**On the allowlist.** GitWyrm's own measurement stands: Copilot returns real model
entitlements only to OAuth apps on an approved client list, and GitWyrm's app is not on it
(29 models with 12 enabled from an approved client, versus ~8 with 0 from GitWyrm's own).
Routing through GitHub's CLI to obtain those entitlements is not circumvention of a
technical restriction in the AUP sense - it is using the client GitHub built and documented
for third-party integration, authenticating as itself. The distinction that matters:
GitWyrm does not impersonate an approved client and does not reuse another client's
credentials. It invokes a first-party tool the way that tool's own docs describe.

Two caveats to keep honest:

- Copilot CLI requires an active Copilot subscription and can be disabled by an org or
  enterprise admin. A run has to fail gracefully when it is switched off, not look broken.
- "Officially supported" describes today's documentation. It is a better position than the
  other two providers, not a permanent guarantee.

### How the Copilot CLI is actually driven

Verified against GitHub's own command reference and ACP server reference (fetched from
`github/docs`, 2026-07-30), not from the summary above. Three things it settles:

**Use ACP over stdio, not `-p`.** `copilot --acp --stdio` starts an Agent Client Protocol
server speaking structured JSON-RPC over stdin/stdout. `-p/--prompt` prints prose meant
for a human to read, and parsing it would put us at the mercy of every wording change. ACP
is a published protocol with named methods, which is the difference between an interface
and a screen-scrape. GitHub documents it for exactly this: "IDE integrations", "CI/CD
pipelines", "custom frontends".

The caveat to hold: ACP support is in **public preview and subject to change**. That is a
reason to isolate it behind `ProviderAgent` -- which we are doing anyway -- not a reason to
prefer the prose path.

**Tool filtering is set when the server starts, not per session.** `--available-tools` and
`--excluded-tools` are server-level options; a client cannot narrow them through
`session/new`. So GitWyrm launches its own server process per run with the bounded tool set
already applied, rather than connecting to a shared one and asking nicely. This suits us:
the guardrails are ours to enforce, and a server we started is one we can kill.

**Auth is checkable without touching credential files.** `copilot login` uses the OAuth
device flow and stores its token in the system credential store; `COPILOT_GITHUB_TOKEN`,
`GH_TOKEN`, and `GITHUB_TOKEN` are honoured in that order, which the docs call out as "most
suitable for headless use such as automation". We read none of these. `copilot version`
reports the installed version for the floor check, and the ACP handshake failing is the
answer to "is this usable", from the tool itself.

One correction to an earlier assumption: there is **no `--allow-all-tools` needed for our
case**, because we are not approving a broad tool set and then hoping. We start the server
with only the tools the bounded set names.

## What bounds a run

### Turn budget: 12, and the user can change it

A run is bounded by **turns** - complete plan/act/observe cycles - not by wall-clock time.
The default is **12**, exposed as a setting so a user who hits it often can raise it.

Turns rather than minutes because the same task should behave the same way on every
provider. A five-minute ceiling gives a fast provider fifteen real steps and a slow one
four, so "didn't finish" would mean different things depending on who the user signed in
with. A turn is a unit of work; a minute is not.

Twelve because the spike measured 10-20 seconds per turn, putting a worst-case run at
roughly three to four minutes. That is long enough to be worth starting and short enough
that a run gone wrong is not left grinding. It is a starting value, not a finding - the
setting exists because the right number is the kind of thing only real use reveals.

A run that spends its budget ends as **didn't finish**, naming the budget as the cause. It
never reports success it did not reach, and never sits there looking like it is still
working.

### Done means the task's checkbox is ticked

A run targets one OpenSpec task. It is done when that task's checkbox is ticked in
`tasks.md`.

This reuses what already exists rather than inventing a completion concept: the checkbox
is in the file, visible in the Desk, already parsed by `openspec/parse.rs`, and already
writable one line at a time by `toggle_task_line`. The user can see the same thing the
loop is checking.

The alternative - requiring a project check to pass - was considered and rejected as the
sole signal. Many spec tasks are documentation, UI copy, or spec text, where no check
proves anything; gating those on a green build would mean they could never complete. A
check remains available to the loop as a *tool* (`run a project check`), so a task whose
own done-means calls for one can still use it. It is just not what defines done.

The obvious risk is that ticking a checkbox is cheap and the model could tick it without
doing the work. Two things sit against that, neither of them in this change: the run
produces a diff the user reviews before keeping it, and the console's keep-or-undo choice
is the actual gate. The checkbox ends the *loop*; the user ends the *run*.

### Still open

Nothing blocking. The remaining terms question is whether to pursue written approval from
Anthropic (contact sales) or OpenAI (App Server known-clients channel) if a subprocess path
for either becomes worth having. Neither is needed to ship.
