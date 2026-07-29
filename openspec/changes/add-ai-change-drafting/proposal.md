# Change: AI-drafted changes and the validate-fix loop

## Why

Creating a spec is the moment teams either adopt spec-driven work or bounce off it.
With an AI configured, "＋ New" grows a Draft-it-for-me path: describe the change in a
sentence, watch the AI read the specs library and draft a proposal, tasks, and deltas -
then review every artifact before a single file is written. The same
review-before-write contract powers the validate-fix loop ("spec check failed → Fix
with AI"). The cardinal rule everywhere: nothing touches disk during generation.

## What Changes

- "Draft it for me" in the new-change flow: description + user-chosen folder name
  (auto-uniqued on collision), staged visible drafting (reads specs library → proposal
  → tasks → deltas), cancellable
- Review screen: per-artifact Keep/Skip cards that genuinely control what is written,
  a live writes-N-files count, Create / Discard; nothing written until Create
- Provenance: created changes start their History with "Drafted with <provider> ·
  reviewed by you"
- Validate-fix loop: a failed spec check offers "Fix with AI", drafting a requirement
  from that change's own proposal for review (Add this delta / Dismiss)

## Impact

- Affected specs: `ai-authoring` (new capability)
- Affected code: new-change modal, drafting driver calls, validation result UI
- Depends on: `add-spec-desk-handoff-actions` (blank flow), `add-ai-provider-surface`
