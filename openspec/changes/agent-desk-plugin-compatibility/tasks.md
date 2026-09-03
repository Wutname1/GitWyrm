# Tasks: plugin compatibility

## 1. Visibility

- [ ] 1.1 For a chosen tool, list the skills and connectors it will load, scoped to the
      session's repository where the tool has repo-local config.
- [ ] 1.2 Report what cannot be enumerated as unknown, naming the tool, rather than
      showing an empty list.
- [ ] 1.3 Show this in the session context panel beside sources and usage.
- [ ] 1.4 Unit tests: a tool with skills and connectors lists both; a tool GitWyrm cannot
      enumerate reports unknown rather than none.

## 2. Per-session exclusion

- [ ] 2.1 Record excluded capabilities on the session, durably, without touching the
      tool's own configuration files.
- [ ] 2.2 Apply exclusions at launch, beside the existing tool denial in
      `cli_agent::denied_tools_for`.
- [ ] 2.3 Per tool, state whether exclusion can be enforced at all; where it cannot, the
      control is absent or disabled with the reason.
- [ ] 2.4 An exclusion applies to one session only; another session on the same tool is
      unaffected.
- [ ] 2.5 Unit tests: an exclusion reaches the launch line; a tool with no mechanism
      reports that it cannot enforce; the tool's config file is byte-identical after.

## 3. Carrying a capability

- [ ] 3.1 Offer to make a skill or connector available to a tool that lacks it, from the
      session, routed through `agent_config`'s existing preview/apply/undo.
- [ ] 3.2 Never introduce a second write path for agent configuration.
- [ ] 3.3 Unit test: carrying goes through the same plan, and can be undone.

## 4. Honesty

- [ ] 4.1 A capability list never implies completeness it does not have.
- [ ] 4.2 The read-only promise is restated per tool where an unenumerable capability
      could carry write ability.
- [ ] 4.3 Unit tests for the wording of each refusal and unknown state.

## 5. Acceptance

- [ ] 5.1 Native: open a chat on Claude Code with real skills installed and confirm the
      list matches what the tool actually loads.
- [ ] 5.2 Native: exclude a connector for one session, run it, and confirm the agent did
      not have it while another session still did.
- [ ] 5.3 Native: confirm the tool's own configuration files are unchanged afterwards.
- [ ] 5.4 Native: on a tool that cannot exclude, confirm the control says so rather than
      appearing to work.
- [ ] 5.5 Record Gate evidence for this package.
