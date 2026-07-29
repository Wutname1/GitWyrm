# ai-provider Spec Delta

## ADDED Requirements

### Requirement: Verified provider state

The Specs feature SHALL read the AI provider from the existing BYO-AI settings and
treat it as configured only after a verified connection check. Stored credentials that
fail verification SHALL show a distinct needs-reconnect state, never a healthy one.

#### Scenario: Stale token

- WHEN a Copilot sign-in exists but returns no usable models
- THEN the chip shows an amber reconnect state, and run entry points explain reconnecting
  or offer the copy-handoff path - never a silent failure

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
window", or raw model IDs. Approved framings: "handoff", "what the AI reads". Cost
hints use plan language ("uses your Copilot plan") and "about" estimates, never
invoice-precise amounts.

#### Scenario: Identity line

- WHEN the identity line renders
- THEN it names the provider and plan in plain words with a change-in-Settings link
