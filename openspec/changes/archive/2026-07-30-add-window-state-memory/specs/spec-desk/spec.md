# spec-desk Spec Delta

## MODIFIED Requirements

### Requirement: Popout window

The Spec Desk SHALL be a separate OS window per repository. The main window SHALL
stay fully usable while the Desk is open. Opening the Desk when one is already
open SHALL focus it. Remembering size and position per repository is specified in
`app-windows`.

#### Scenario: Second monitor

- WHEN the user opens the Spec Desk and moves it to another monitor
- THEN both windows work simultaneously, and the Desk reopens where it was left

#### Scenario: Keep on top

- WHEN the user toggles Keep on top
- THEN the Desk floats above other windows until toggled off, and the toggle state is visible

#### Scenario: Show main window

- WHEN the user clicks Show main window in the Desk titlebar
- THEN the main window is focused and the Desk stays open
