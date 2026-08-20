# Agent Desk Spec Delta

## ADDED Requirements

### Requirement: Agent Desk reuses the Spec Desk window

Agent Desk SHALL replace and expand the existing Spec Desk second window. Opening through
legacy or new entry points SHALL focus one window rather than create a third.

#### Scenario: Legacy entry

- WHEN the user opens Spec Desk from an older deep link
- THEN the Agent Desk window opens with the same repository/change context

### Requirement: Sessions are dense and chat-first

The left column SHALL show one-line session rows suitable for dozens of daily sessions
and SHALL support Recent, Project, and Diff groupings.

#### Scenario: Long title

- WHEN a session title exceeds the row width
- THEN it ellipsizes on one line and remains available through accessible text

### Requirement: The source remains visible

The conversation SHALL show what started the session above the transcript, including a
cached state when the live source is unavailable.

#### Scenario: Source changed

- WHEN the live issue/PR/OpenSpec source changed since launch
- THEN the banner states that and lets the user view current and launch information

### Requirement: Message history supports quick return

A transcript-edge history rail SHALL expose prior user messages on hover and keyboard
focus in a popup at least half the conversation width. Choosing one SHALL visibly jump to
the matching message.

#### Scenario: Keyboard jump

- WHEN the user focuses the rail and chooses a message
- THEN the transcript scrolls to it, focus moves there, and the row flashes visibly

### Requirement: Mode and team are separate choices

Agent Desk SHALL expose Ask, Plan, and Auto authority separately from Solo and Lead +
helpers execution shape.

#### Scenario: Plan with lead

- WHEN Plan and Lead + helpers are selected
- THEN a graph may be drafted but no helper starts until the user chooses Start

### Requirement: Context and usage are honest

The right panel SHALL default to Context when no graph exists and SHALL omit usage values
the provider does not expose rather than display zero or invent estimates.

#### Scenario: No plan limit

- WHEN a provider reports session use but not an overall limit
- THEN session use is shown and no overall-limit row appears
