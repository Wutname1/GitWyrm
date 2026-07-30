# Change: The Desk detail area (tabs, overview, tasks)

## Why

The center of the Desk is where a change is actually read and worked: the proposal, the
agreed behavior, the history, and above all the task list. This change fills the shell
from `add-spec-desk-window` with the Overview / Proposal / Spec deltas / History tabs
and the interactive task list from the mockup.

## What Changes

- Change header: breadcrumb, human title, one-line goal, status pill
- Overview tab: progress card (ring, counts, signal + hint lines), latest-activity
  card, and the change-package grid (Proposal / Spec deltas / Design / Tasks cards)
- Proposal tab (Why / What changes / Impact), Spec deltas tab (kind-badged cards with
  count on the tab), History tab (attributed entries)
- Grouped task list with clickable checkboxes (write-back), "Ready now" highlight,
  per-task hover action, and honest empty states

## Impact

- Affected specs: `spec-desk`
- Affected code: Desk React tree (new components), markdown rendering utility
- Depends on: `add-spec-desk-window`
