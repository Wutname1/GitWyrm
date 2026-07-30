# ai-provider Specification

## Purpose
TBD - created by archiving change add-ai-provider-surface. Update Purpose after archive.
## Requirements
### Requirement: Verified provider state

The Specs feature SHALL read the AI provider from the existing BYO-AI settings and
treat it as configured only after a verified connection check. Stored credentials that
fail verification SHALL show a distinct needs-reconnect state, never a healthy one.

The check SHALL ask the provider whether it is usable rather than inferring it from the
presence of a credential file. Both known providers prove why: a stale Copilot token
still looks signed in while returning zero enabled models, and the Claude Code spike hit
an account whose credential file held a complete OAuth record but whose sign-in lacked
the scope needed to generate - `claude doctor` reported it, a file-existence check would
have called it ready and failed later.

#### Scenario: Stale token

- WHEN a Copilot sign-in exists but returns no usable models
- THEN the chip shows an amber reconnect state, and run entry points explain reconnecting
  or offer the copy-handoff path - never a silent failure

#### Scenario: Credentials present but not usable

- WHEN a provider CLI holds complete-looking credentials but reports itself not logged in
  or lacking the scope it needs
- THEN GitWyrm believes the provider's own answer and shows needs-reconnect

### Requirement: Provider chip

The Desk titlebar SHALL show a provider chip: provider and model when configured, a
muted "AI · not set up" otherwise. Clicking it SHALL open Settings → AI. The chip is
the single place provider identity lives in the chrome.

#### Scenario: Configured

- WHEN Copilot with a chosen model is verified
- THEN the chip reads "✦ GitHub Copilot · <model>" with a healthy indicator

### Requirement: Rail hierarchy follows the provider

When an AI is configured, the rail's next-task card SHALL make "Run this task with AI"
primary and "Ask about this change" secondary, with a plain identity line naming the
provider, model, and plan, linking to Settings. External handoff (Copy task handoff /
Open in opencode / Open in VS Code) and the handoff preview SHALL move into an
always-visible collapsed "Prefer your own editor?" section whose expanded state
persists across UI updates. When no AI is configured, the rail SHALL be exactly the
pre-AI baseline with copy handoff primary.

#### Scenario: Configured hierarchy

- WHEN an AI is configured and a task is open
- THEN Run with AI is the primary button and external handoff is one disclosure-click away

#### Scenario: Disclosure persistence

- WHEN the user expands Prefer your own editor and any state updates re-render the rail
- THEN the section stays expanded

#### Scenario: Run already active on this change

- WHEN a run is working on the selected change
- THEN the card offers "The AI is working on task N - watch" instead of a second run button

### Requirement: No-AI state is complete, hidden not disabled

With no AI configured, every AI entry point SHALL be hidden (not greyed out), the copy
workflow SHALL be fully functional, and exactly one quiet, dismissible "Connect an AI"
card SHALL appear at the rail bottom - never a modal, never the primary action, never
in the main window.

#### Scenario: Invitation

- WHEN no AI is configured
- THEN the rail bottom shows "Run tasks right here / Connect an AI" naming Copilot,
  Anthropic, or a local model, with a Connect button opening Settings → AI

#### Scenario: Nothing dead

- WHEN no AI is configured
- THEN no disabled sparkle buttons appear anywhere in either window

### Requirement: Main window follows the provider

When an AI is configured, the main-window spec card's primary action SHALL become
"Run next task with AI" (opening the Desk and starting the run in one motion), with
copy handoff as secondary. The Desk status bar SHALL show an AI segment (ready /
working on task N / needs your answer) only when configured.

#### Scenario: One-motion start

- WHEN the user clicks Run next task with AI in the main window
- THEN the Desk opens (or focuses) with the run already starting

### Requirement: Plain-language AI copy

Primary UI copy for AI features SHALL avoid jargon: no "prompt", "tokens", "context
window", or raw model IDs. Approved framings: "handoff", "what the AI reads".

GitWyrm SHALL NOT show a price, a cost estimate, or a token count for AI work. The
Claude Code CLI spike found its envelope reports a `total_cost_usd` that does not
correspond to what a subscription user pays, and an `input_tokens` that plainly excludes
system and cached content. Numbers we cannot stand behind are worse than no numbers:
plan language ("uses your Copilot plan") is the most GitWyrm claims.

#### Scenario: Identity line

- WHEN the identity line renders
- THEN it names the provider and plan in plain words with a change-in-Settings link

#### Scenario: No invented prices

- WHEN a provider's output includes a cost or token field
- THEN GitWyrm does not surface it as a price or a usage figure

