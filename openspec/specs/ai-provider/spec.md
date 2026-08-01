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

### Requirement: The AI can be turned off without removing it

A user SHALL be able to turn AI features off while keeping their provider configured.
Turning the AI off SHALL NOT remove, revoke, or invalidate any credential, and turning it
back on SHALL NOT require signing in again.

While off, every AI surface SHALL be exactly the no-AI baseline already specified: AI entry
points hidden rather than greyed out, copy handoff primary in the rail, the main-window
spec card's primary action back to copy handoff, and no AI segment in the Desk status bar.

The off state SHALL be remembered across restarts and SHALL be distinct from having no
provider configured, so the app never tells a user who has an AI set up that they have not
set one up.

#### Scenario: Turning it off

- WHEN a user with a configured provider turns the AI off
- THEN AI entry points disappear from both windows, the copy-handoff workflow is fully
  functional, and no disabled sparkle buttons are left behind

#### Scenario: Credentials survive

- WHEN the AI is turned off and then on again
- THEN the same provider and model are in use, with no sign-in step

#### Scenario: Off is not unconfigured

- WHEN the AI is off and a provider is configured
- THEN the chip reads as off rather than "not set up", and the "Connect an AI" invitation
  card does not appear

#### Scenario: Off survives a restart

- WHEN the user turns the AI off and reopens GitWyrm
- THEN it is still off

### Requirement: The chip is the control

The Desk provider chip SHALL act on a click rather than only describing state. Its behavior
SHALL depend on how many providers are configured:

- With exactly one configured provider, clicking SHALL toggle the AI between on and off.
- With more than one, clicking SHALL open a menu listing every configured provider with its
  model, marking the one in use, then an entry to turn the AI off, and last an entry opening
  Settings → AI. Choosing a provider SHALL make it the default.
- With none configured, the chip SHALL behave as it does today - naming that no AI is set up
  and pointing at Settings → AI.

The chip SHALL state what a click will do, so a user never has to click to find out.

#### Scenario: Single provider toggles

- WHEN one provider is configured and the user clicks the chip
- THEN the AI turns off, the chip says so immediately, and clicking again turns it back on

#### Scenario: Several providers open a menu

- WHEN two or more providers are configured and the user clicks the chip
- THEN a menu lists each with its model, marks the one in use, and offers turning the AI off
  and opening the AI settings

#### Scenario: Reaching the settings from the menu

- WHEN the user chooses the settings entry at the bottom of the chip menu
- THEN the main window comes forward on Settings → AI, where providers are added and removed

#### Scenario: Settings is always reachable from the chip

- WHEN the chip is clicked in any state - off, ready, needs-reconnect, or nothing configured
- THEN a way to reach Settings → AI is one click away, so adding or removing a provider never
  requires hunting for it

#### Scenario: Switching provider from the chip

- WHEN the user picks a different provider from the chip menu
- THEN that provider becomes the default, the chip names it, and the next run and the next
  commit message use it - with no restart

#### Scenario: A second provider is added

- WHEN a user who had one provider configures another
- THEN the chip becomes a menu without the user being told to look for it

#### Scenario: Nothing configured

- WHEN no provider is configured and the user clicks the chip
- THEN it explains where to add one, exactly as before this change

### Requirement: One switch, both windows, one source of truth

The on/off state and the provider choice SHALL be the same setting the AI settings view
reads and writes, and every AI feature SHALL resolve them through the shared selection path
rather than reading the setting itself.

Changing either from the chip SHALL update the Desk rail, the Desk status bar, the
main-window spec card, and the AI settings view together, without a restart.

#### Scenario: Settings agrees with the chip

- WHEN the AI is turned off from the chip
- THEN Settings → AI shows it as off, with the provider still listed as configured

#### Scenario: Turned off in Settings

- WHEN the AI is turned off from the settings view instead
- THEN the Desk chip and rail reflect it immediately in the already-open Desk window

#### Scenario: Features cannot disagree

- WHEN the AI is off
- THEN commit-message generation, the commit-generation flow, and Spec Desk runs all report
  as unavailable together - none of them offers an action the others have hidden

### Requirement: Turning off does not silently abandon a run

Turning the AI off SHALL NOT cancel a run that is already working. Stopping a run is the run
controls' job. While a run finishes after the AI has been turned off, the chip SHALL say the
run is still finishing rather than claiming the AI is simply off, and the run's own surface
SHALL stay reachable until it ends.

#### Scenario: Off during a run

- WHEN a run is working on a change and the user turns the AI off
- THEN the run continues, its progress stays visible, and the chip says a run is still
  finishing

#### Scenario: After that run ends

- WHEN the run finishes
- THEN the AI is off, and no new run can be started until it is turned back on

