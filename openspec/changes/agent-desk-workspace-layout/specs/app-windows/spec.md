# app-windows Spec Delta

## MODIFIED Requirements

### Requirement: Windows remember their placement

GitWyrm SHALL remember each OS window's size, position, and maximized state and restore it
the next time that window opens. The main window and the single app-wide Agent Desk window
SHALL be remembered independently. Agent Desk placement SHALL NOT be keyed per repository.

#### Scenario: Agent Desk reopens where it was left

- WHEN the user moves Agent Desk to a second monitor, resizes it, and closes it
- THEN reopening Agent Desk from any repository restores that size and position

#### Scenario: Repository switch does not move Agent Desk

- WHEN Agent Desk is open and the user starts a chat from a different repository
- THEN the same window keeps its placement while selecting the requested chat

#### Scenario: Main window survives a restart

- WHEN the user maximizes the main window and quits
- THEN the next launch opens maximized

