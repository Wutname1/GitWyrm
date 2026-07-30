# Tasks

## 1. State

- [ ] 1.1 Persist an `ai_enabled` flag alongside the default-provider setting, defaulting
      to on so an existing install is unchanged
- [ ] 1.2 `useAiSelection`: an off selection is `ready: false` while still reporting the
      configured provider list, so consumers see not-configured but the chip and Settings
      can still name what is set up
- [ ] 1.3 `useSpecAi`: an `off` state distinct from `none`, with `configured: false` so
      every existing entry-point gate keeps working untouched
- [ ] 1.4 Turning off never writes to the credential store

## 2. Chip

- [ ] 2.1 Single-provider click toggles on/off, with the tooltip naming what the click does
- [ ] 2.2 Multi-provider click opens a menu: each configured provider with its model, the
      current one marked, then "Turn AI off", then "AI settings" last
- [ ] 2.3 Menu selection sets the default provider through the shared path
- [ ] 2.4 Settings entry focuses the main window on Settings → AI (the Desk has no settings
      view of its own); reachable from every chip state, not just the menu
- [ ] 2.5 Off label and styling: "AI · off", visually distinct from "AI · not set up" and
      from the amber reconnect state
- [ ] 2.6 Zero-provider behavior unchanged

## 3. Both windows follow

- [ ] 3.1 Rail returns to the exact pre-AI baseline when off (copy handoff primary,
      no hidden-but-present sparkle buttons)
- [ ] 3.2 "Connect an AI" invitation card stays hidden when off-but-configured
- [ ] 3.3 Main-window spec card primary action reverts to copy handoff
- [ ] 3.4 Desk status bar drops its AI segment
- [ ] 3.5 Settings → AI shows the same switch and reflects chip changes live, and vice
      versa, in an already-open Desk

## 4. Runs

- [ ] 4.1 Toggling off leaves an in-flight run running and its surface reachable
- [ ] 4.2 Chip reports "run still finishing" rather than plain off during that window
- [ ] 4.3 No new run can start while off

## 5. Verify

- [ ] 5.1 `npm run typecheck`
- [ ] 5.2 In a real native window: one-provider toggle, two-provider menu switch, off state
      across both windows, restart persistence
