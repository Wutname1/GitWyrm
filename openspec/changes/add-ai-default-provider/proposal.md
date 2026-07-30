# Change: Set up several AIs and pick a default

## Why

GitWyrm stores one `ai_provider` and one `ai_model`, so configuring a second provider
replaces the first as the thing every feature uses. That is already awkward for commit
messages, and it does not survive the run engine: a user may reasonably want a fast cheap
model for commit messages and a stronger one for running tasks, or a Copilot subscription
for one and an API key for the other.

The credential layer is already per-provider - `auth.json` holds a map keyed by provider
id, with both API-key and OAuth entries - so several providers can already be signed in at
once. What is missing is a way to say which one is *the* one, and a single place every AI
feature reads that from.

## What Changes

- Settings lists every configured provider rather than treating the chosen one as the only
  one, and each keeps its own selected model
- One provider is marked **default**. The default is what AI features use unless a feature
  is deliberately pointed elsewhere: commit-message generation, the commit-generation
  flow, and (once it exists) the Spec Desk run engine all read the same setting
- Removing the default provider promotes another rather than leaving the app with a
  dangling default
- Configuring the first provider makes it the default, so a single-provider user never has
  to know this setting exists

## Impact

- Affected specs: `ai-provider` (extends the capability added by
  `add-ai-provider-surface`)
- Affected code: `src-tauri/src/settings.rs` (a default marker and per-provider model
  selection, replacing the single `ai_provider`/`ai_model` pair), the AI settings view,
  `useSpecAi`, and the commit-message paths that read the provider today
- Depends on: `add-ai-provider-surface`
- Depended on by: `add-ai-agent-engine` - the engine reads the default rather than
  carrying its own provider choice

## Migration

An existing install has `ai_provider`/`ai_model` set. Those become the default provider and
its model on first read, so nobody is asked to reconfigure. The old fields stay readable
for one release rather than being deleted outright.
