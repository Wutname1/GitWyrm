# app-windows Spec Delta

## ADDED Requirements

### Requirement: Windows remember their placement

GitWyrm SHALL remember each window's size, position, and maximized state per
window, and restore them the next time that window opens. The main window and
each repository's Spec Desk are remembered independently.

#### Scenario: Desk reopens where it was left

- WHEN the user moves the Spec Desk to a second monitor, resizes it, and closes it
- THEN reopening that repository's Desk restores that size and position

#### Scenario: Per-repository placement

- WHEN two repositories' Desks are arranged differently
- THEN each reopens with its own placement, not the other's

#### Scenario: Main window survives a restart

- WHEN the user maximizes the main window and quits
- THEN the next launch opens maximized

### Requirement: A restored window is always reachable

A saved position SHALL never place a window where the user cannot see or move it.
When the saved position is not on any currently-attached display, GitWyrm SHALL
place the window on an attached display instead.

#### Scenario: Monitor no longer attached

- WHEN a Desk was saved on a second monitor that is now unplugged
- THEN it opens on an attached display at a usable size, not off-screen

#### Scenario: First open has a sane default

- WHEN a window has no saved placement yet
- THEN it opens at its default size, centered

## MODIFIED Requirements

### Requirement: Popout window

The Spec Desk SHALL be a separate OS window per repository. The main window SHALL
stay fully usable while the Desk is open. Opening the Desk when one is already
open SHALL focus it. Size and position are remembered per repository by the
window-state handling described in this capability.

#### Scenario: Second monitor

- WHEN the user opens the Spec Desk and moves it to another monitor
- THEN both windows work simultaneously, and the Desk reopens where it was left

#### Scenario: Keep on top

- WHEN the user toggles Keep on top
- THEN the Desk floats above other windows until toggled off, and the toggle state is visible

#### Scenario: Show main window

- WHEN the user clicks Show main window in the Desk titlebar
- THEN the main window is focused and the Desk stays open
