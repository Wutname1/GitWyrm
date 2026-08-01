# Tasks

## 1. State

- [x] 1.1 Persist an `ai_enabled` flag alongside the default-provider setting, defaulting
      to on so an existing install is unchanged
- [x] 1.2 `useAiSelection`: an off selection is `ready: false` while still reporting the
      configured provider list, so consumers see not-configured but the chip and Settings
      can still name what is set up
- [x] 1.3 `useSpecAi`: an `off` state distinct from `none`, with `configured: false` so
      every existing entry-point gate keeps working untouched
- [x] 1.4 Turning off never writes to the credential store

## 2. Chip

- [x] 2.1 Single-provider click toggles on/off, with the tooltip naming what the click does
- [x] 2.2 Multi-provider click opens a menu: each configured provider with its model, the
      current one marked, then "Turn AI off", then "AI settings" last
- [x] 2.3 Menu selection sets the default provider through the shared path
- [x] 2.4 Settings entry focuses the main window on Settings → AI (the Desk has no settings
      view of its own); reachable from every chip state, not just the menu
- [x] 2.5 Off label and styling: "AI · off", visually distinct from "AI · not set up" and
      from the amber reconnect state
- [x] 2.6 Zero-provider behavior unchanged

## 3. Both windows follow

- [x] 3.1 Rail returns to the exact pre-AI baseline when off (copy handoff primary,
      no hidden-but-present sparkle buttons) - falls out of `ai.configured` being false
- [x] 3.2 "Connect an AI" invitation card stays hidden when off-but-configured (it gates on
      `state === 'none'`, which off is not)
- [x] 3.3 Main-window spec card primary action reverts to copy handoff
- [x] 3.4 Desk status bar drops its AI segment
- [x] 3.5 Settings → AI shows the same switch and reflects chip changes live, and vice
      versa, in an already-open Desk

## 4. Runs

- [x] 4.1 Toggling off leaves an in-flight run running and its surface reachable
- [x] 4.2 Chip reports "AI · finishing run" rather than plain off during that window
- [x] 4.3 No new run can start while off

## 5. Verify

- [x] 5.1 `npm run typecheck`
- [x] 5.2 `cargo check` and `cargo test`
- [x] 5.3 In a real native window: one-provider toggle, two-provider menu switch, off state
      across both windows, restart persistence. **Still needs a human at the window.**
      The dev build runs clean and the wiring was audited: `ChipButton` is a
      `forwardRef` rendering a bare `<button>`, and `TooltipHint` now wraps
      *outside* `DropdownMenuTrigger` - nesting it inside made the tooltip the
      asChild target and would have silently stopped the menu opening, which is
      the exact failure this task predicted. Restart persistence is covered by
      `the_ai_switch_round_trips_through_settings_json`. What remains is purely
      visual: that the menu actually drops down, and that the off state reads
      right in both windows.
