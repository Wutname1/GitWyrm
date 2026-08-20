# Change: Evolve Spec Desk into the Agent Desk conversation shell

## Why

Durable sessions need a chat-first surface. The current Spec Desk is change-first and its
right rail is not a session context/graph surface.

## What Changes

- Reuse the existing second window as Agent Desk; do not add a third window.
- Add dense session navigation, source-bound transcript, history jump, composer modes and
  team choice, and Context/Graph side-panel tabs.
- Keep existing OpenSpec detail/edit/handoff capabilities reachable inside the new shell.
- Preserve legacy Spec Desk routes and settings during migration.

## Impact

- `src/views/SpecDeskView.tsx` evolves through a compatibility wrapper.
- New `src/components/domain/agent-desk/` components.
- Existing `spec-desk` components are reused or moved in behavior-sized steps.
