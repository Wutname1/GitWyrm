# Change: Add a lightweight skill and MCP configuration sync manager

## Why

Users repeat the same skill and connector setup across agent clients. GitWyrm can discover
existing configuration and safely copy selected items without becoming another marketplace.

## What Changes

- Read skills/MCP configuration through external-client adapters.
- Show differences per item and destination client.
- Preview, back up, atomically apply, and undo per-item copies.
- Protect secrets and preserve unknown client-specific configuration.

## Impact

- Extends adapter configuration reads and adds client-specific merge writers only after
  fixtures prove safe behavior.
- Adds Agent Setup view inside Agent Desk.
