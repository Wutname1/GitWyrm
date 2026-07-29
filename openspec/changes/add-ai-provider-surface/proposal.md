# Change: AI provider surface and the no-AI fallback

## Why

Before any run can happen, the Desk needs to know whether a working AI is configured
and to reshape itself honestly around that answer. The trust anchor is a provider chip
that reflects a *verified* connection (GitWyrm has a known failure mode where a stale
Copilot token looks signed-in but returns zero models). The fallback rule: with no AI,
the Desk is still complete - copy handoffs stay primary, and AI entry points are
hidden, not greyed out.

## What Changes

- Provider state from the existing BYO-AI settings, with a verified-connection check
- Provider chip in the Desk titlebar (provider + model; muted "AI · not set up")
- Action-rail reordering when configured: Run with AI primary, Ask secondary, external
  handoff in an always-visible collapsed "Prefer your own editor?" section whose open
  state survives re-renders
- Quiet "Connect an AI" card in the no-AI state (never a modal, never primary, dismissible)
- Provider identity line under the run button; main-window spec card primary swaps to
  "Run next task with AI"; status-bar AI segment

## Impact

- Affected specs: `ai-provider` (new capability)
- Affected code: Desk rail, Desk titlebar, main-window spec card, status bars,
  BYO-AI settings module (read-only reuse)
- Depends on: `add-spec-desk-handoff-actions`, the BYO-AI provider work
