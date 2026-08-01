# Change: Turn the AI on and off from the Desk chip

## Why

The provider chip is already the Desk's trust anchor - it answers "is an AI involved
here, and which one?" - but it is read-only. Clicking it only produces a toast telling the
user to go find Settings in the other window. Two things a user wants to do from that chip
are currently impossible without leaving the Desk:

- **Turn the AI off.** A user with a provider configured gets AI-first defaults everywhere:
  Run with AI is primary in the rail, external handoff is folded into a disclosure, the
  main-window card leads with "Run next task with AI". There is no way to say "not right
  now" short of removing the provider's credentials, which is destructive and has to be
  undone by signing in again. The no-AI path is already complete and fully specified - it
  just cannot be reached on purpose.
- **Switch providers.** With several providers configured, changing which one runs the next
  task means opening the main window, finding Settings → AI, and changing the default -
  when the chip naming the current one is right there.

## What Changes

- The chip becomes the control, not just the readout. Its behavior scales with how many
  providers are configured:
  - **One provider**: clicking toggles the AI on and off for the whole app. Off is the
    existing no-AI state - AI entry points hidden, copy handoffs primary - reached
    deliberately rather than only by having nothing set up.
  - **Several providers**: clicking opens a menu listing each configured provider with its
    model, the current one marked, then "Turn AI off", and last a way into Settings → AI.
    Picking a provider makes it the default; the next run and the next commit message use
    it. The settings entry keeps adding and removing providers one click from the chip -
    the menu covers switching between what is already set up, not managing the list.
  - **None configured**: unchanged - it says "AI · not set up" and points at Settings.
- Turned-off is a distinct, remembered state, not the same as having no provider. The chip
  says so ("AI · off"), and turning it back on restores the provider that was in use.
- Turning off never touches credentials. Nothing has to be signed in to again.
- Both windows follow immediately: the rail hierarchy, the main-window card's primary
  action, and the Desk status bar's AI segment all flip with no restart.

## Impact

- Affected specs: `ai-provider` (extends `add-ai-provider-surface` and
  `add-ai-default-provider`)
- Affected code: `AiProviderChip.tsx` (menu + toggle), `useSpecAi` / `useAiSelection` (an
  off state that resolves to not-configured for every consumer), settings persistence for
  the enabled flag and the remembered provider, `AiSettings.tsx` (same switch, one source
  of truth)
- Depends on: `add-ai-provider-surface`, `add-ai-default-provider`
- Not affected: the run engine, which reads the resolver rather than the setting, so an
  in-flight run is unaffected by a later toggle

## Open question

Turning the AI off while a run is working: this change specifies that the toggle does not
cancel it - stopping a run stays the run controls' job - and that the chip shows the run
is still finishing. If in-app testing shows that reads as the switch not working, the
alternative is to make the toggle unavailable while a run is active, with a reason.
