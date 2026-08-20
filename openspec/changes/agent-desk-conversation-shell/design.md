# Design

## Layout

- Left column: 218-282 px, virtual one-line session rows, Recent/Project/Diff grouping.
- Center: source banner, transcript, history rail, and composer.
- Right: Context before graph; Graph when selected/running.
- At narrow widths, the sidebar becomes a drawer and the right panel stacks or becomes a
  sheet. Controls never overlap or shrink below readable size.

The reference is `docs/agent-desk/agent-desk-mockup.html`.

## Migration

Route both `spec-desk` and `agent-desk` to the new view. First preserve existing OpenSpec
behavior in a source/detail view. Rename/move files later in mechanical-only commits.

## Message history

The rail contains user-message positions only. Hover and focus reveal a popup at least
half the conversation width. Clicking scrolls, focuses, and visibly flashes the message.

## Usage honesty

Provider usage fields are optional. Unknown is omitted, not zero. Estimated values are
marked in details. The panel is collapsible and remembers its state.
