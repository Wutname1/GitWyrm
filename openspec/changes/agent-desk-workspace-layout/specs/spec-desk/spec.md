# spec-desk Spec Delta

## MODIFIED Requirements

### Requirement: Popout window

Agent Desk SHALL be one app-wide second OS window. The main window SHALL stay fully usable
while Agent Desk is open. Opening from any repository, issue, pull request, or legacy Spec
Desk entry point while Agent Desk is open SHALL focus the existing window and select the
requested source or session. GitWyrm SHALL NOT create one Agent Desk per repository or a
third deep-work window.

#### Scenario: Work across two repositories

- WHEN the user opens Agent Desk from repository A and then starts a chat from repository B
- THEN the same Agent Desk window is focused and can show both repository sessions

#### Scenario: Keep on top

- WHEN the user toggles Keep on top
- THEN Agent Desk floats above other windows until toggled off, and the toggle state is visible

#### Scenario: Show main window

- WHEN the user clicks Show main window in the Agent Desk titlebar
- THEN the main window is focused and Agent Desk stays open

