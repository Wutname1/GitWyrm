# app-windows Specification

## Purpose
TBD - created by archiving change add-window-state-memory. Update Purpose after archive.
## Requirements
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

