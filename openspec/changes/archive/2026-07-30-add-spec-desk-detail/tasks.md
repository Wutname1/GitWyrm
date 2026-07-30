# Tasks

## 1. Header and tabs

- [x] 1.1 Crumb (`openspec / changes / <id>`), title, goal line, status pill
- [x] 1.2 Tab bar: Overview · Proposal · Spec deltas (count badge) · History

## 2. Overview

- [x] 2.1 Progress card: ring with percent, "X of Y done", signal line with correct
      singular/plural grammar, hint line per state (in build / draft / no tasks / complete)
- [x] 2.2 Latest-activity card: three most recent history entries with who and when
- [x] 2.3 Change-package grid: Proposal / Spec deltas / Design / Tasks cards with
      present/missing marks and one-line descriptions

## 3. Tabs

- [x] 3.1 Proposal tab renders Why / What changes / Impact
- [x] 3.2 Spec deltas tab: cards with ADDED/MODIFIED/REMOVED badge, target spec file,
      requirement and scenario text; empty state
- [x] 3.3 History tab: icon + text + "when · who" rows; AI entries attributed
      "with <provider> · reviewed by you"
- [x] 3.4 All rendered markdown is inert (no HTML/script injection from file content)

## 4. Task list

- [x] 4.1 Groups from tasks.md headings; checkbox click writes the file
- [x] 4.2 Next open task highlighted "Ready now"; done tasks struck through
- [x] 4.3 Per-task hover action: Copy handoff (baseline) / Run with AI (once AI ships)
- [x] 4.4 Empty state: "No tasks yet — add them in tasks.md (plain markdown)."

## 5. Verify

- [x] 5.1 Toggle every state combination (draft, partial, complete, no deltas)
- [x] 5.2 Malformed proposal renders raw without breaking the tab bar
- [x] 5.3 Manual check in a native window
