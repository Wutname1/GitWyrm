# Design: plugin compatibility

## The decision: host, do not define

"Plugin framework" could mean two things, and only one of them is worth building.

Defining a GitWyrm plugin format means a manifest, a runtime, a sandbox, a permission
model, and an author willing to package for a client with no users. Every one of those
is a large piece of work whose value depends entirely on adoption GitWyrm does not have.

Hosting means: the agent tools already load skills and MCP servers, so let them, show
the person what got loaded, and let them narrow it per session. The capability surface is
whatever those tools already support, which is considerably larger than anything this
project would ship on its own.

This package is the second one.

## What already exists

`agent_config` reads five clients and now writes four. It already knows where skills
live (folders with a `SKILL.md`), where connectors live (a member of a JSON object, in
whichever of several shapes a file uses), and how to copy either safely with hash
gating, backup and undo.

What it does not do is connect any of that to a *run*. The inventory is a browser. A
session starts an agent and that agent independently loads whatever its own config says.
Those two facts have never met.

## The three things to add

**Visibility.** Before a run, say what this tool will load. This is mostly reading what
`agent_config` already reads, scoped to the chosen client, and showing it where the
session context panel already shows sources and usage.

The honest part is what cannot be enumerated. Claude Code's plugin-enabled skills, a
connector defined somewhere GitWyrm does not read: those must be reported as unknown.
An empty list that means "we did not look" is worse than no list.

**Per-session exclusion.** Turn one off for this chat without editing the tool's file.
This has to be a launch-time argument, because that is the only place GitWyrm controls
what an agent loads. `cli_agent::denied_tools_for` is the existing precedent: it decides
per execution what the CLI is allowed, and a capability exclusion belongs beside it.

The honest part here is bigger. The tools differ, exactly as they differ on tool denial:
Claude Code takes `--disallowedTools` and its own settings flags; Codex bounds a whole
session; opencode offers nothing at all. Where a tool cannot exclude, the control must
be absent or disabled with the reason. This is the same shape as
`registry::Denial::None` refusing read-only work on opencode, and for the same reason:
a promise nothing enforces is worse than an admitted gap.

**Carrying.** Making a capability available to a tool that lacks it is already built.
It is `agent_config`'s copy flow, and reusing it is non-negotiable: a second way to
write agent configuration would need its own gating, backup and undo, and would drift.

## Why not run MCP servers ourselves

GitWyrm could host an MCP server and offer it to every tool. That is a real option and
this package deliberately does not take it: it makes GitWyrm a runtime, with a lifecycle
to manage, ports or pipes to secure, and failures to explain. The agent tools already do
this. Adding a second host would be defining a plugin format by another name.

## Open questions for implementation

- Whether exclusion should be remembered per session or per session-and-tool. A person
  who switches a chat from Claude to Codex may or may not expect their exclusions to
  follow.
- Whether "unknown capabilities" should block a read-only promise. If GitWyrm cannot
  enumerate what a tool loads, it cannot promise a read-only session loaded nothing that
  writes; today the launch-time denial covers that, but a skill that shells out is worth
  thinking about.
- Whether to show capabilities for a tool that is not installed, so the person can see
  what they would gain by installing it.
