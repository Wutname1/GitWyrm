# Change: Host the skills and connectors other agent tools already have

## Why

People arrive with agent tooling already set up: skills written for Claude Code, MCP
connectors configured in VS Code, servers defined in opencode. Today GitWyrm can read
those, list them, copy a connector between apps, and copy a skill folder. What it cannot
do is *run* them: a skill sitting in the inventory is a file GitWyrm knows about, not a
capability an Agent Desk run can use.

The wrong fix is a GitWyrm plugin format. That asks every author to package again for a
client with no users yet, and it means a new sandbox to secure, which is the largest
possible surface for the smallest possible gain.

The right fix is to host what exists. An agent already loads its own skills and its own
MCP servers; GitWyrm's job is to stop stripping them, to make what is loaded visible,
and to let the person decide per session what a run may reach.

## What Changes

- Make what a run loads visible: which skills and which connectors this session's agent
  actually has, named, before it starts.
- Let the person turn one off for a session without editing the tool's own config.
- Carry a skill or connector into a run that would not otherwise see it, using the copy
  machinery that already exists rather than a second mechanism.
- State plainly, per tool, what GitWyrm can and cannot control: the tools differ, and
  pretending they are uniform is how a promise gets broken quietly.

## Impact

- Extends `src-tauri/src/agent_config/` (already reads five clients, writes four) and
  the Agent Setup screen.
- Extends the session context panel, which already shows sources and usage, to show
  capabilities.
- Touches launch arguments per tool in `src-tauri/src/ai/agent/`: this is where a
  capability is actually included or excluded.
- Adds no runtime, no sandbox, and no GitWyrm-specific plugin format.
